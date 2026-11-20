//! The EQ 7-Band window: one framed column per band, in signal order (Input, HPF, the five
//! tone bands, LPF, Output), with rotary knobs for the gains, frequencies and Qs, a switch for
//! each filter's on/off, and the slope choices below. Each band has its own colour, which also
//! marks it on the response curve above.

use super::{Color32, SoundApp, Stroke, Tokens, Ui, json, mono};
use crate::widgets::{ParamKnob, switch};
use soundcraft_dsp::{ParamInfo, PluginInfo, Unit};
use soundcraft_model::{Insert, TrackId};

/// One framed column of the window.
pub(super) struct Band {
    /// Shown at the top of the frame.
    pub title: &'static str,
    /// The frame and curve-marker colour; `None` for the plain Input and Output frames.
    pub color: Option<Color32>,
    /// The parameter of the band's on/off switch.
    pub switch: Option<&'static str>,
    /// Parameters shown as knobs, top to bottom.
    pub knobs: &'static [&'static str],
    /// A parameter shown as a list below the knobs.
    pub choice: Option<&'static str>,
}

const fn rgb(r: u8, g: u8, b: u8) -> Option<Color32> {
    Some(Color32::from_rgb(r, g, b))
}

pub(super) const EQ7_BANDS: [Band; 9] = [
    Band { title: "Input", color: None, switch: None, knobs: &["input_gain"], choice: None },
    Band { title: "HPF", color: rgb(150, 120, 220), switch: Some("hpf_on"), knobs: &["hpf_freq"], choice: Some("hpf_slope") },
    Band { title: "Low", color: rgb(220, 92, 80), switch: None, knobs: &["low_shelf_gain", "low_shelf_freq", "low_shelf_q"], choice: None },
    Band { title: "Low Mid", color: rgb(230, 150, 60), switch: None, knobs: &["low_mid_gain", "low_mid_freq", "low_mid_q"], choice: None },
    Band { title: "Mid", color: rgb(200, 190, 70), switch: None, knobs: &["mid_gain", "mid_freq", "mid_q"], choice: None },
    Band { title: "High Mid", color: rgb(80, 180, 110), switch: None, knobs: &["high_mid_gain", "high_mid_freq", "high_mid_q"], choice: None },
    Band { title: "High", color: rgb(70, 160, 220), switch: None, knobs: &["high_shelf_gain", "high_shelf_freq", "high_shelf_q"], choice: None },
    Band { title: "LPF", color: rgb(120, 130, 230), switch: Some("lpf_on"), knobs: &["lpf_freq"], choice: Some("lpf_slope") },
    Band { title: "Output", color: None, switch: None, knobs: &["output_gain"], choice: None },
];

/// Width of a band's contents, inside its frame.
const COLUMN: f32 = 86.0;

/// The window's default width: every column in one row.
pub(super) const EQ7_WIDTH: f32 = 9.0 * (COLUMN + 14.0 + 8.0) + 24.0;

/// Where a band acts on the curve: its frequency, its gain if it has one, and whether it is on.
pub(super) fn marker(band: &Band, value: impl Fn(&str) -> f32) -> Option<(f32, Option<f32>, bool)> {
    band.color?;
    let freq = band.knobs.iter().find(|k| k.ends_with("_freq"))?;
    let gain = band.knobs.iter().find(|k| k.ends_with("_gain")).map(|g| value(g));
    let on = band.switch.is_none_or(|s| value(s) >= 0.5);
    Some((value(freq), gain, on))
}

/// The knob's label under it: the parameter's name without the band's prefix ("LF Gain" → "Gain").
fn short_name(p: &ParamInfo) -> &'static str {
    p.name.split_once(' ').map_or("", |(_, rest)| rest)
}

pub(super) fn eq7_panel(app: &mut SoundApp, ui: &mut Ui, tid: TrackId, slot: usize, info: &PluginInfo, ins: &Insert) {
    let t = Tokens::current();
    let param = |id: &str| info.params.iter().find(|p| p.id == id);
    let value = |p: &ParamInfo| p.clamp(ins.params.get(p.id).copied().unwrap_or(p.default));
    let mut changes: Vec<(&'static str, f32)> = Vec::new();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for band in &EQ7_BANDS {
            let col = band.color.unwrap_or(t.text_dim);
            // Band colours are mid-tones; titles darken on a light theme to stay readable.
            let title_col = match band.color {
                Some(c) if !ui.visuals().dark_mode => crate::widgets::shade(c, -0.4),
                Some(c) => c,
                None => ui.visuals().strong_text_color(),
            };
            let frame = egui::Frame::new()
                .fill(col.gamma_multiply(0.07))
                .stroke(Stroke::new(1.0, col.gamma_multiply(0.75)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(6));
            frame.show(ui, |ui| {
                ui.set_width(COLUMN);
                ui.vertical(|ui| {
                    let on = band.switch.and_then(param).is_none_or(|p| value(p) >= 0.5);
                    // The title, and the switch to its right.
                    ui.horizontal(|ui| {
                        ui.set_min_height(18.0);
                        let title = egui::RichText::new(crate::localization::text(band.title)).strong().color(title_col);
                        ui.label(title);
                        if let Some(p) = band.switch.and_then(param) {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let mut state = on;
                                if switch(ui, &mut state, col, crate::localization::text(p.name)).changed() {
                                    changes.push((p.id, if state { 1.0 } else { 0.0 }));
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                    // A filter that is off keeps its settings, shown faded.
                    if !on {
                        ui.multiply_opacity(0.45);
                    }
                    ui.vertical_centered(|ui| {
                        for p in band.knobs.iter().filter_map(|id| param(id)) {
                            let v = value(p);
                            let gain = p.unit == Unit::Db;
                            let mut pos = p.to_normalized(v);
                            let name = crate::localization::text(p.name);
                            let shown = p.format(v);
                            let size = if gain && band.color.is_some() { 46.0 } else { 34.0 };
                            let knob = ParamKnob::new(&mut pos, p.to_normalized(p.default), name, &shown).size(size).bipolar(gain).color(col);
                            if knob.show(ui).changed() {
                                changes.push((p.id, p.from_normalized(pos)));
                            }
                            let short = short_name(p);
                            if !short.is_empty() {
                                ui.label(egui::RichText::new(crate::localization::text(short)).small().color(t.text_dim));
                            }
                            ui.label(egui::RichText::new(shown).font(mono(11.0)));
                            ui.add_space(2.0);
                        }
                        if let Some(p) = band.choice.and_then(param) {
                            if let Some(x) = choice(ui, tid, slot, p, value(p)) {
                                changes.push((p.id, x));
                            }
                            ui.label(egui::RichText::new(crate::localization::text(short_name(p))).small().color(t.text_dim));
                        }
                    });
                });
            });
        }
    });
    for (id, x) in changes {
        let _ = app.engine.execute_merged(
            "mix.insert_param",
            &json!({"track": tid.0, "slot": slot, "param": id, "value": x}),
            &format!("param:{}:{slot}:{id}", tid.0),
        );
    }
}

/// A list for a choice parameter; an Alt-click returns it to its default without opening it.
/// Returns the new value when it changed.
fn choice(ui: &mut Ui, tid: TrackId, slot: usize, p: &ParamInfo, v: f32) -> Option<f32> {
    let mut idx = v.round().max(0.0) as usize;
    let selected = p.choices.get(idx).copied().unwrap_or("?");
    let r = craft_ui::ComboBox::from_id_salt(("pc", tid.0, slot, p.id))
        .width(COLUMN)
        .selected_text(egui::RichText::new(crate::localization::text(selected)).size(11.0))
        .show_ui(ui, |ui| {
            for (i, c) in p.choices.iter().enumerate() {
                ui.selectable_value(&mut idx, i, crate::localization::text(c));
            }
        });
    crate::widgets::named_text_field(r.response.clone(), crate::localization::text(p.name));
    let mut x = idx as f32;
    if r.response.clicked() && ui.input(|i| i.modifiers.alt) {
        x = p.default;
        egui::Popup::close_all(ui.ctx());
    }
    ((x - v).abs() > f32::EPSILON).then_some(x)
}
