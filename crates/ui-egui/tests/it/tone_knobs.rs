//! Tone EQ: bass and treble are rotary knobs in the plugin window; turning one sets the
//! parameter, and a double-click returns it to flat.

use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use soundcraft_model::TrackId;
use soundcraft_ui_egui::{Services, SoundApp};

fn harness() -> (Harness<'static, SoundApp>, TrackId) {
    let mut engine = soundcraft_engine::Engine::default();
    engine.execute("track.new", &json!({"name": "Mix", "format": "stereo"})).unwrap();
    let t = engine.session().tracks[0].id;
    engine.execute("mix.insert", &json!({"track": t.0, "slot": 0, "plugin": "tone_eq"})).unwrap();
    let mut app = SoundApp::new(engine, None, Services::default());
    app.ui.plugin_windows.push((t, 0));
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    (h, t)
}

fn param(h: &Harness<'_, SoundApp>, t: TrackId, id: &str) -> f32 {
    let tr = h.state().engine.session().track(t).unwrap();
    tr.mixer.inserts[0].as_ref().unwrap().params.get(id).copied().unwrap_or(0.0)
}

fn press(h: &mut Harness<'_, SoundApp>, pos: Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

fn move_to(h: &mut Harness<'_, SoundApp>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
}

#[test]
fn bass_knob_turns_and_double_click_returns_to_flat() {
    let (mut h, t) = harness();
    let at = h.get_by_label("Bass (drag; double-click resets)").rect().center();
    // Dragging up turns the knob clockwise: more bass.
    move_to(&mut h, at);
    press(&mut h, at, true);
    for i in 1..=6 {
        move_to(&mut h, at - vec2(0.0, 30.0 * i as f32 / 6.0));
    }
    press(&mut h, at - vec2(0.0, 30.0), false);
    h.run_steps(2);
    let boosted = param(&h, t, "bass");
    assert!(boosted > 3.0 && boosted <= 15.0, "bass {boosted}");
    assert_eq!(param(&h, t, "treble"), 0.0, "only the bass knob moved");

    // A double-click returns to flat. A second of frames first: egui counts a click within 0.6 s
    // of the one before last as a triple-click.
    h.run_steps(60);
    for pressed in [true, false, true, false] {
        h.event(Event::PointerMoved(at));
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(2);
    assert!(param(&h, t, "bass").abs() < 1e-4, "bass {}", param(&h, t, "bass"));
}

#[test]
fn knobs_replace_the_bass_and_treble_sliders() {
    let (h, _) = harness();
    assert!(h.query_by_label("Treble (drag; double-click resets)").is_some());
    // The frequency choices and the output gain stay in the grid.
    for label in ["Bass Freq", "Treble Freq", "Output"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
}
