use egui::{Color32, RichText, Ui};

use crate::midi::types::{LearnTarget, MidiMsgType, TriggerAction};
use crate::midi::MidiSystem;
use crate::params::{ParamDef, ParamStore, ParamValue};
use crate::ui::accessibility::focus::draw_focus_ring;

const MIDI_BLUE: Color32 = Color32::from_rgb(0x60, 0xA0, 0xE0);
const MIDI_AMBER: Color32 = Color32::from_rgb(0xE0, 0xA0, 0x40);

/// Draw a MIDI mapping badge for a parameter.
/// Shows: "M" (unmapped), pulsing "..." (learning), or "CC42" badge (mapped).
fn draw_midi_badge(ui: &mut Ui, midi: &mut MidiSystem, param_name: &str) {
    let is_learning = midi.learn_target == Some(LearnTarget::Param(param_name.to_string()));
    let is_mapped = midi.config.params.contains_key(param_name);

    if is_learning {
        let t = ui.input(|i| i.time) as f32;
        let alpha = ((t * 4.0).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
        let color = Color32::from_rgba_unmultiplied(0xE0, 0xA0, 0x40, (alpha * 255.0) as u8);
        if ui
            .button(RichText::new(" ... ").color(color).small())
            .on_hover_text("Click to cancel MIDI learn")
            .clicked()
        {
            midi.cancel_learn();
        }
        ui.ctx().request_repaint(); // keep animating
    } else if is_mapped {
        let mapping = &midi.config.params[param_name];
        let label = format_mapping_label(mapping.msg_type, mapping.cc);
        if ui
            .button(RichText::new(&label).color(MIDI_BLUE).small())
            .on_hover_text("Click to re-learn, right-click to clear")
            .clicked()
        {
            midi.start_learn(LearnTarget::Param(param_name.to_string()));
        }
        if ui
            .button(RichText::new("\u{00D7}").small()) // × symbol
            .on_hover_text("Clear MIDI mapping")
            .clicked()
        {
            midi.clear_param_mapping(param_name);
        }
    } else {
        if ui
            .button(RichText::new(" M ").weak().small())
            .on_hover_text("MIDI learn — click then move a knob")
            .clicked()
        {
            midi.start_learn(LearnTarget::Param(param_name.to_string()));
        }
    }
}

/// Draw a MIDI mapping badge for a trigger action.
fn draw_trigger_badge(ui: &mut Ui, midi: &mut MidiSystem, action: TriggerAction) {
    let is_learning = midi.learn_target == Some(LearnTarget::Trigger(action));
    let is_mapped = midi.config.triggers.contains_key(&action);

    if is_learning {
        let t = ui.input(|i| i.time) as f32;
        let alpha = ((t * 4.0).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
        let color = Color32::from_rgba_unmultiplied(0xE0, 0xA0, 0x40, (alpha * 255.0) as u8);
        if ui
            .button(RichText::new(" ... ").color(color).small())
            .on_hover_text("Click to cancel MIDI learn")
            .clicked()
        {
            midi.cancel_learn();
        }
        ui.ctx().request_repaint();
    } else if is_mapped {
        let mapping = &midi.config.triggers[&action];
        let label = format_mapping_label(mapping.msg_type, mapping.cc);
        if ui
            .button(RichText::new(&label).color(MIDI_BLUE).small())
            .on_hover_text("Click to re-learn")
            .clicked()
        {
            midi.start_learn(LearnTarget::Trigger(action));
        }
        if ui
            .button(RichText::new("\u{00D7}").small())
            .on_hover_text("Clear MIDI mapping")
            .clicked()
        {
            midi.clear_trigger_mapping(action);
        }
    } else {
        if ui
            .button(RichText::new(" M ").weak().small())
            .on_hover_text("MIDI learn — click then press a button")
            .clicked()
        {
            midi.start_learn(LearnTarget::Trigger(action));
        }
    }
}

fn format_mapping_label(msg_type: MidiMsgType, cc: u8) -> String {
    match msg_type {
        MidiMsgType::Cc => format!(" CC{cc} "),
        MidiMsgType::Note => format!(" N{cc} "),
    }
}

pub fn draw_param_panel(ui: &mut Ui, store: &mut ParamStore, midi: &mut MidiSystem) {
    if store.defs.is_empty() {
        ui.label("No parameters for current effect");
    } else {
        let defs = store.defs.clone();

        for def in &defs {
            match def {
                ParamDef::Float {
                    name,
                    min,
                    max,
                    ..
                } => {
                    let current = match store.get(name) {
                        Some(ParamValue::Float(v)) => *v,
                        _ => *min,
                    };
                    let mut val = current;

                    // Row 1: name (left) + MIDI badge (right)
                    ui.horizontal(|ui| {
                        ui.strong(name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_midi_badge(ui, midi, name);
                        });
                    });

                    // Row 2: slider controls
                    ui.horizontal(|ui| {
                        let step = (max - min) * 0.01;
                        if ui.small_button("-").clicked() {
                            val = (val - step).max(*min);
                        }

                        let response = ui.add(
                            egui::Slider::new(&mut val, *min..=*max)
                                .clamping(egui::SliderClamping::Always),
                        );
                        draw_focus_ring(ui, &response);

                        if ui.small_button("+").clicked() {
                            val = (val + step).min(*max);
                        }

                        if ui.small_button("R").on_hover_text("Reset").clicked() {
                            store.reset(name);
                            return;
                        }
                    });

                    if val != current {
                        store.set(name, ParamValue::Float(val));
                    }
                }
                ParamDef::Color { name, .. } => {
                    let current = match store.get(name) {
                        Some(ParamValue::Color(c)) => *c,
                        _ => [1.0, 1.0, 1.0, 1.0],
                    };
                    let mut color = current;

                    ui.horizontal(|ui| {
                        ui.label(name);
                        let response = ui.color_edit_button_rgba_unmultiplied(&mut color);
                        draw_focus_ring(ui, &response);

                        if ui.small_button("R").on_hover_text("Reset").clicked() {
                            store.reset(name);
                            return;
                        }
                    });

                    if color != current {
                        store.set(name, ParamValue::Color(color));
                    }
                }
                ParamDef::Bool { name, .. } => {
                    let current = match store.get(name) {
                        Some(ParamValue::Bool(b)) => *b,
                        _ => false,
                    };
                    let mut val = current;

                    ui.horizontal(|ui| {
                        let response = ui.checkbox(&mut val, name);
                        draw_focus_ring(ui, &response);

                        if ui.small_button("R").on_hover_text("Reset").clicked() {
                            store.reset(name);
                            return;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_midi_badge(ui, midi, name);
                        });
                    });

                    if val != current {
                        store.set(name, ParamValue::Bool(val));
                    }
                }
                ParamDef::Point2D {
                    name, min, max, ..
                } => {
                    let current = match store.get(name) {
                        Some(ParamValue::Point2D(p)) => *p,
                        _ => *min,
                    };
                    let mut val = current;

                    ui.label(name);
                    ui.horizontal(|ui| {
                        ui.label("X");
                        let rx = ui.add(egui::Slider::new(&mut val[0], min[0]..=max[0]));
                        draw_focus_ring(ui, &rx);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y");
                        let ry = ui.add(egui::Slider::new(&mut val[1], min[1]..=max[1]));
                        draw_focus_ring(ui, &ry);

                        if ui.small_button("R").on_hover_text("Reset").clicked() {
                            store.reset(name);
                            return;
                        }
                    });

                    if val != current {
                        store.set(name, ParamValue::Point2D(val));
                    }
                }
            }
            ui.add_space(4.0);
        }

        ui.add_space(8.0);
        if ui.button("Reset All").clicked() {
            store.reset_all();
        }
    }

    // MIDI Triggers section
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(2.0);
    ui.strong("MIDI Triggers");
    ui.add_space(4.0);

    for action in TriggerAction::ALL {
        ui.horizontal(|ui| {
            ui.label(action.display_name());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                draw_trigger_badge(ui, midi, *action);
            });
        });
    }
}
