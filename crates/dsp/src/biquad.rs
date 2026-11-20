//! Second-order IIR filters designed from the RBJ "Audio EQ Cookbook" formulas, plus first-order
//! low/high-pass and shelving sections (bilinear transform) for 6 dB/oct slopes.

use std::f64::consts::PI;

use crate::util::{flush, sane_sr};

/// Filter response shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
    AllPass,
    /// First-order (6 dB/oct) low-pass; `q` and `gain_db` are ignored.
    LowPass1,
    /// First-order (6 dB/oct) high-pass; `q` and `gain_db` are ignored.
    HighPass1,
    /// First-order (6 dB/oct) low shelf: `gain_db` below, unity above, half the gain at `freq`;
    /// `q` is ignored. A cut mirrors the boost of the same size.
    LowShelf1,
    /// First-order (6 dB/oct) high shelf: `gain_db` above, unity below, half the gain at `freq`;
    /// `q` is ignored. A cut mirrors the boost of the same size.
    HighShelf1,
}

/// Normalized biquad coefficients (`a0 = 1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for Coeffs {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Coeffs {
    /// Passes the signal unchanged.
    pub const IDENTITY: Coeffs = Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// Designs a filter. All inputs are sanitized: frequency is clamped to 1 Hz .. 0.49·sr,
    /// Q to 0.025 .. 100 and gain to ±48 dB.
    pub fn design(kind: FilterType, freq: f32, q: f32, gain_db: f32, sample_rate: f32) -> Coeffs {
        let sr = f64::from(sane_sr(sample_rate));
        let f = if freq.is_finite() { f64::from(freq).clamp(1.0, sr * 0.49) } else { 1000.0 };
        let q = if q.is_finite() { f64::from(q).clamp(0.025, 100.0) } else { std::f64::consts::FRAC_1_SQRT_2 };
        let g = if gain_db.is_finite() { f64::from(gain_db).clamp(-48.0, 48.0) } else { 0.0 };
        let w0 = 2.0 * PI * f / sr;
        let (sw, cw) = w0.sin_cos();
        let alpha = sw / (2.0 * q);
        let a = 10f64.powf(g / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterType::LowPass => ((1.0 - cw) / 2.0, 1.0 - cw, (1.0 - cw) / 2.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::HighPass => ((1.0 + cw) / 2.0, -(1.0 + cw), (1.0 + cw) / 2.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Notch => (1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::AllPass => (1.0 - alpha, -2.0 * cw, 1.0 + alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterType::Peak => (1.0 + alpha * a, -2.0 * cw, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cw, 1.0 - alpha / a),
            FilterType::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cw + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
                    a * ((a + 1.0) - (a - 1.0) * cw - s),
                    (a + 1.0) + (a - 1.0) * cw + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cw),
                    (a + 1.0) + (a - 1.0) * cw - s,
                )
            }
            FilterType::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cw + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
                    a * ((a + 1.0) + (a - 1.0) * cw - s),
                    (a + 1.0) - (a - 1.0) * cw + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cw),
                    (a + 1.0) - (a - 1.0) * cw - s,
                )
            }
            FilterType::LowPass1 | FilterType::HighPass1 => {
                // Bilinear transform of 1/(s+1) with prewarped cutoff.
                let k = (w0 / 2.0).tan();
                let norm = 1.0 / (1.0 + k);
                let a1 = (k - 1.0) * norm;
                return if kind == FilterType::LowPass1 {
                    Coeffs { b0: (k * norm) as f32, b1: (k * norm) as f32, b2: 0.0, a1: a1 as f32, a2: 0.0 }
                } else {
                    Coeffs { b0: norm as f32, b1: -norm as f32, b2: 0.0, a1: a1 as f32, a2: 0.0 }
                };
            }
            FilterType::LowShelf1 | FilterType::HighShelf1 => {
                // Bilinear transform (prewarped at `freq`) of (s + √G)/(s + 1/√G) for the low
                // shelf and (√G·s + 1)/(s/√G + 1) for the high shelf: the zero and pole sit
                // symmetrically about the corner, so `freq` is the half-gain point.
                let k = (w0 / 2.0).tan();
                let r = 10f64.powf(g / 40.0);
                let (b0, b1, a0, a1) = if kind == FilterType::LowShelf1 {
                    (1.0 + k * r, k * r - 1.0, 1.0 + k / r, k / r - 1.0)
                } else {
                    (r + k, k - r, 1.0 / r + k, k - 1.0 / r)
                };
                (b0, b1, 0.0, a0, a1, 0.0)
            }
        };
        let c = Coeffs { b0: (b0 / a0) as f32, b1: (b1 / a0) as f32, b2: (b2 / a0) as f32, a1: (a1 / a0) as f32, a2: (a2 / a0) as f32 };
        if [c.b0, c.b1, c.b2, c.a1, c.a2].iter().all(|v| v.is_finite()) { c } else { Coeffs::IDENTITY }
    }

    /// Magnitude response in dB at `freq` Hz.
    pub fn magnitude_db(&self, freq: f32, sample_rate: f32) -> f32 {
        let sr = f64::from(sane_sr(sample_rate));
        let f = if freq.is_finite() { f64::from(freq).clamp(0.0, sr * 0.5) } else { 0.0 };
        let w = 2.0 * PI * f / sr;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let (b0, b1, b2, a1, a2) = (f64::from(self.b0), f64::from(self.b1), f64::from(self.b2), f64::from(self.a1), f64::from(self.a2));
        let nr = b0 + b1 * c1 + b2 * c2;
        let ni = -(b1 * s1 + b2 * s2);
        let dr = 1.0 + a1 * c1 + a2 * c2;
        let di = -(a1 * s1 + a2 * s2);
        let num = nr * nr + ni * ni;
        let den = (dr * dr + di * di).max(1e-30);
        let db = 10.0 * (num / den).max(1e-30).log10();
        db.max(-300.0) as f32
    }
}

/// A biquad section with its own state (transposed direct form II).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Biquad {
    pub coeffs: Coeffs,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(coeffs: Coeffs) -> Self {
        Self { coeffs, z1: 0.0, z2: 0.0 }
    }

    /// Designs a new filter (see [`Coeffs::design`]).
    pub fn design(kind: FilterType, freq: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        Self::new(Coeffs::design(kind, freq, q, gain_db, sample_rate))
    }

    /// Re-designs the coefficients, keeping the state (for smooth sweeps).
    pub fn set(&mut self, kind: FilterType, freq: f32, q: f32, gain_db: f32, sample_rate: f32) {
        self.coeffs = Coeffs::design(kind, freq, q, gain_db, sample_rate);
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let c = &self.coeffs;
        let y = c.b0 * x + self.z1;
        self.z1 = flush(c.b1 * x - c.a1 * y + self.z2);
        self.z2 = flush(c.b2 * x - c.a2 * y);
        if y.is_finite() {
            y
        } else {
            self.reset();
            0.0
        }
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        for s in buf {
            *s = self.process_sample(*s);
        }
    }

    /// Magnitude response in dB at `freq` Hz, for drawing curves.
    pub fn magnitude_db(&self, freq: f32, sample_rate: f32) -> f32 {
        self.coeffs.magnitude_db(freq, sample_rate)
    }
}

/// Butterworth Q values for cascaded sections making an `order`-th order filter (2, 4).
pub(crate) fn butterworth_qs(order: usize) -> &'static [f32] {
    match order {
        4 => &[0.541_196_1, 1.306_563],
        _ => &[std::f32::consts::FRAC_1_SQRT_2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_gain_at_center() {
        let c = Coeffs::design(FilterType::Peak, 1000.0, 1.0, 6.0, 48_000.0);
        assert!((c.magnitude_db(1000.0, 48_000.0) - 6.0).abs() < 0.01);
        assert!(c.magnitude_db(20.0, 48_000.0).abs() < 0.1);
    }

    #[test]
    fn lowpass_minus3_at_cutoff() {
        let c = Coeffs::design(FilterType::LowPass, 1000.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, 48_000.0);
        assert!((c.magnitude_db(1000.0, 48_000.0) + 3.01).abs() < 0.05);
        let c1 = Coeffs::design(FilterType::HighPass1, 1000.0, 0.0, 0.0, 48_000.0);
        assert!((c1.magnitude_db(1000.0, 48_000.0) + 3.01).abs() < 0.05);
    }

    #[test]
    fn first_order_shelves_are_gentle_and_symmetric() {
        let sr = 48_000.0;
        for (kind, far, near) in [(FilterType::LowShelf1, 5.0, 20_000.0), (FilterType::HighShelf1, 20_000.0, 5.0)] {
            let fc = if kind == FilterType::LowShelf1 { 100.0 } else { 2500.0 };
            let boost = Coeffs::design(kind, fc, 0.0, 12.0, sr);
            let cut = Coeffs::design(kind, fc, 0.0, -12.0, sr);
            // Half the gain at the corner, (almost) the full gain far away, unity on the other side.
            assert!((boost.magnitude_db(fc, sr) - 6.0).abs() < 0.05, "{kind:?}");
            assert!(boost.magnitude_db(far, sr) > 11.0, "{kind:?}");
            assert!(boost.magnitude_db(near, sr).abs() < 0.5, "{kind:?}");
            // A cut is the exact mirror of the boost.
            for f in [20.0, 200.0, 1000.0, 2500.0, 8000.0, 16_000.0] {
                assert!((boost.magnitude_db(f, sr) + cut.magnitude_db(f, sr)).abs() < 0.01, "{kind:?} at {f}");
            }
            // No steeper than a first-order slope: under 6 dB per octave anywhere.
            let mut f = 20.0f32;
            while f < 10_000.0 {
                assert!((boost.magnitude_db(f * 2.0, sr) - boost.magnitude_db(f, sr)).abs() < 6.0, "{kind:?} at {f}");
                f *= 2.0;
            }
        }
        // Zero gain is a pass-through.
        let flat = Coeffs::design(FilterType::HighShelf1, 5000.0, 0.0, 0.0, sr);
        assert!(flat.magnitude_db(15_000.0, sr).abs() < 1e-4);
    }

    #[test]
    fn hostile_inputs() {
        let c = Coeffs::design(FilterType::Peak, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0);
        assert!(c.b0.is_finite());
        let mut b = Biquad::new(c);
        assert!(b.process_sample(f32::NAN) == 0.0);
    }
}
