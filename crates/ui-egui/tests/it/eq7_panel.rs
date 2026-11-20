//! The EQ 7-Band window: a framed column per band, knobs for the gains, frequencies and Qs,
//! switches for the filters' on/off, and lists for their slopes.

use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use soundcraft_dsp::ParamInfo;
use soundcraft_model::TrackId;
use soundcraft_ui_egui::{Services, SoundApp};

/// Kick's EQ 7-Band window (the demo session's first insert on Kick).
fn kick_eq() -> (Harness<'static, SoundApp>, TrackId) {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    let kick = app.engine.session().track_by_name("Kick").unwrap().id;
    app.ui.plugin_windows = vec![(kick, 0)];
    let proof = std::env::var_os("SOUNDCRAFT_EQ7_PROOF_DIR").is_some();
    let mut builder = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_step_dt(1.0 / 60.0);
    if proof {
        builder = builder.wgpu();
    }
    let mut h = builder.build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    (h, kick)
}

fn param(id: &str) -> &'static ParamInfo {
    soundcraft_dsp::plugin_info("eq_7band").unwrap().params.iter().find(|p| p.id == id).unwrap()
}

fn value(h: &Harness<'_, SoundApp>, t: TrackId, id: &str) -> f32 {
    let tr = h.state().engine.session().track(t).unwrap();
    tr.mixer.inserts[0].as_ref().unwrap().params.get(id).copied().unwrap_or(param(id).default)
}

fn press(h: &mut Harness<'_, SoundApp>, pos: Pos2, pressed: bool, modifiers: Modifiers) {
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers });
    h.run_steps(1);
}

fn move_to(h: &mut Harness<'_, SoundApp>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
}

fn hold(h: &mut Harness<'_, SoundApp>, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.run_steps(1);
}

/// Drags the knob named `name` `dy` points up in six steps with `modifiers` held.
fn turn(h: &mut Harness<'_, SoundApp>, name: &str, dy: f32, modifiers: Modifiers) {
    let at = h.get_by_role_and_label(Role::Slider, name).rect().center();
    hold(h, modifiers);
    move_to(h, at);
    press(h, at, true, modifiers);
    for i in 1..=6 {
        move_to(h, at - vec2(0.0, dy * i as f32 / 6.0));
    }
    press(h, at - vec2(0.0, dy), false, modifiers);
    hold(h, Modifiers::NONE);
    h.run_steps(2);
}

fn undo_steps(h: &Harness<'_, SoundApp>) -> usize {
    h.state().engine.undo_history().len()
}

#[test]
fn every_parameter_has_its_own_control_and_every_band_its_frame_title() {
    let (h, _) = kick_eq();
    for p in soundcraft_dsp::plugin_info("eq_7band").unwrap().params {
        let role = match p.unit {
            soundcraft_dsp::Unit::Toggle => Role::Switch,
            soundcraft_dsp::Unit::Choice => Role::ComboBox,
            _ => Role::Slider,
        };
        assert!(h.query_by_role_and_label(role, p.name).is_some(), "{} has no {role:?}", p.name);
    }
    for title in ["Input", "HPF", "Low", "Low Mid", "Mid", "High Mid", "High", "LPF", "Output"] {
        assert!(h.query_all_by_label(title).next().is_some(), "no {title} frame");
    }
    if let Some(dir) = std::env::var_os("SOUNDCRAFT_EQ7_PROOF_DIR") {
        let mut h = h;
        let kick = h.state().engine.session().track_by_name("Kick").unwrap().id;
        for (id, v) in [("hpf_on", 1.0), ("hpf_freq", 60.0), ("low_shelf_gain", 3.0), ("mid_gain", -4.0), ("mid_q", 1.5), ("high_shelf_gain", 2.5)] {
            h.state_mut().engine.execute("mix.insert_param", &json!({"track": kick.0, "slot": 0, "param": id, "value": v})).unwrap();
        }
        h.run_steps(30);
        h.render().unwrap().save(std::path::Path::new(&dir).join("eq7-panel.png")).unwrap();
        h.state_mut().ui.theme = soundcraft_ui_egui::theme::ThemeMode::Light;
        h.run_steps(5);
        h.render().unwrap().save(std::path::Path::new(&dir).join("eq7-panel-light.png")).unwrap();
    }
}

#[test]
fn turning_a_gain_knob_moves_that_gain_only_and_ctrl_turns_it_a_tenth_as_far() {
    let (mut h, kick) = kick_eq();
    let all =
        |h: &Harness<'_, SoundApp>| soundcraft_dsp::plugin_info("eq_7band").unwrap().params.iter().map(|p| value(h, kick, p.id)).collect::<Vec<_>>();
    let before = all(&h);
    let start = value(&h, kick, "low_shelf_gain");
    turn(&mut h, "LF Gain", 40.0, Modifiers::NONE);
    let plain = value(&h, kick, "low_shelf_gain") - start;
    assert!(plain > 2.0, "dragging up did not raise LF Gain: {plain}");
    let after = all(&h);
    let moved: Vec<_> = before.iter().zip(&after).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i).collect();
    assert_eq!(moved.len(), 1, "another parameter moved: {moved:?}");
    let mid = value(&h, kick, "low_shelf_gain");
    turn(&mut h, "LF Gain", 40.0, Modifiers::COMMAND);
    let fine = value(&h, kick, "low_shelf_gain") - mid;
    assert!((0.08..=0.12).contains(&(fine / plain)), "Ctrl-turn moved {fine}, a plain turn {plain}");
}

#[test]
fn a_frequency_knob_turns_evenly_along_its_log_scale() {
    let (mut h, kick) = kick_eq();
    let p = param("mid_freq");
    let start = p.to_normalized(value(&h, kick, p.id));
    turn(&mut h, "MF Freq", 40.0, Modifiers::NONE);
    let moved = p.to_normalized(value(&h, kick, p.id)) - start;
    // About 200 points for the whole travel.
    assert!((0.15..=0.25).contains(&moved), "40 points moved MF Freq {moved} of its travel");
}

#[test]
fn double_click_and_alt_click_return_a_knob_to_its_default_in_one_undo_step() {
    let (mut h, kick) = kick_eq();
    let p = param("high_mid_gain");
    turn(&mut h, "HMF Gain", 40.0, Modifiers::NONE);
    let boosted = value(&h, kick, p.id);
    assert_ne!(boosted, p.default);
    h.state_mut().engine.end_merge();
    let at = h.get_by_role_and_label(Role::Slider, "HMF Gain").rect().center();
    // egui counts a click within 0.6 s of the one before last as a triple-click.
    h.run_steps(60);
    let steps = undo_steps(&h);
    for pressed in [true, false, true, false] {
        h.event(Event::PointerMoved(at));
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(2);
    assert_eq!(value(&h, kick, p.id), p.default, "a double-click did not return HMF Gain to flat");
    assert_eq!(undo_steps(&h), steps + 1, "the double-click is not one undo step");
    h.state_mut().engine.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(value(&h, kick, p.id), boosted, "undo did not bring HMF Gain back");

    h.run_steps(60);
    hold(&mut h, Modifiers::ALT);
    move_to(&mut h, at);
    press(&mut h, at, true, Modifiers::ALT);
    press(&mut h, at, false, Modifiers::ALT);
    hold(&mut h, Modifiers::NONE);
    assert_eq!(value(&h, kick, p.id), p.default, "an Alt-click did not return HMF Gain to flat");
}

#[test]
fn an_alt_drag_leaves_a_knob_where_it_is() {
    let (mut h, kick) = kick_eq();
    let before = value(&h, kick, "input_gain");
    turn(&mut h, "Input", 40.0, Modifiers::ALT);
    assert_eq!(value(&h, kick, "input_gain"), before);
}

#[test]
fn the_hpf_switch_turns_the_filter_on_and_off() {
    let (mut h, kick) = kick_eq();
    let was = value(&h, kick, "hpf_on");
    let at = h.get_by_role_and_label(Role::Switch, "HPF").rect().center();
    move_to(&mut h, at);
    press(&mut h, at, true, Modifiers::NONE);
    press(&mut h, at, false, Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(value(&h, kick, "hpf_on"), 1.0 - was, "the HPF switch did not flip");
    h.state_mut().engine.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(value(&h, kick, "hpf_on"), was, "undo did not flip the HPF back");
}
