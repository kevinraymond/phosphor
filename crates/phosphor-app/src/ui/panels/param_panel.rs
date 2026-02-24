use egui::Ui;

use crate::params::{ParamDef, ParamStore, ParamValue};
use crate::ui::accessibility::focus::draw_focus_ring;

pub fn draw_param_panel(ui: &mut Ui, store: &mut ParamStore) {
    if store.defs.is_empty() {
        ui.label("No parameters for current effect");
        return;
    }

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

                ui.horizontal(|ui| {
                    ui.label(name);

                    // Step buttons for WCAG 2.5.7 dragging alternatives
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
        ui.add_space(2.0);
    }

    ui.add_space(8.0);
    if ui.button("Reset All").clicked() {
        store.reset_all();
    }
}
