use egui::{Color32, CornerRadius, RichText, Stroke, Ui, Vec2};

use crate::preset::PresetStore;
use crate::ui::theme::tokens::*;

const COLS: usize = 3;
const WARNING_COLOR: Color32 = Color32::from_rgb(0xE0, 0x60, 0x40);

pub fn draw_preset_panel(ui: &mut Ui, store: &PresetStore) {
    // Compact save row
    let mut name = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(egui::Id::new("preset_save_name")))
        .unwrap_or_default();

    // "Update" button when dirty and a preset is loaded
    if store.dirty {
        if let Some(current_name) = store.current_name() {
            let current_name = current_name.to_string();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("*{}", current_name))
                        .size(SMALL_SIZE)
                        .color(WARNING_COLOR),
                );
                if ui
                    .button(RichText::new("Update").size(SMALL_SIZE).strong())
                    .on_hover_text("Save changes to current preset")
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("save_preset"), current_name.clone());
                    });
                }
            });
            ui.add_space(2.0);
        }
    }

    ui.horizontal(|ui| {
        let save_width = 40.0;
        let spacing = ui.spacing().item_spacing.x;
        let text_width = (ui.available_width() - save_width - spacing).max(1.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut name)
                .desired_width(text_width)
                .hint_text("Name...")
                .font(egui::FontId::proportional(SMALL_SIZE)),
        );
        let save_btn = ui.add_enabled(
            !name.trim().is_empty(),
            egui::Button::new(RichText::new("SAVE").size(SMALL_SIZE).strong()),
        );
        if save_btn.clicked()
            || (response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !name.trim().is_empty())
        {
            let trimmed = name.trim().to_string();
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("save_preset"), trimmed);
            });
            name.clear();
        }
    });

    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("preset_save_name"), name));

    if store.presets.is_empty() {
        ui.label(RichText::new("No presets").size(SMALL_SIZE).color(DARK_TEXT_SECONDARY));
        return;
    }

    ui.add_space(4.0);

    // Read pending_delete state from temp data
    let now = ui.input(|i| i.time);
    let pending_delete: Option<(usize, f64)> = ui
        .ctx()
        .data_mut(|d| d.get_temp(egui::Id::new("pending_delete_preset")));

    // Expire after 3 seconds
    let pending_delete = pending_delete.filter(|(_, t)| now - t < 3.0);

    let available_width = ui.available_width();
    let gap = 4.0;
    let total_gaps = (COLS - 1) as f32 * gap;
    let btn_width = ((available_width - total_gaps) / COLS as f32).max(40.0);
    let btn_height = 22.0;

    let mut new_pending: Option<(usize, f64)> = pending_delete;

    let presets: Vec<_> = store.presets.iter().enumerate().collect();
    for row in presets.chunks(COLS) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(i, (pname, _)) in row {
                let is_current = store.current_preset == Some(i);
                let is_armed = pending_delete.map_or(false, |(idx, _)| idx == i);

                let (fill, text_color, stroke) = if is_armed {
                    (WARNING_COLOR, Color32::WHITE, Stroke::NONE)
                } else if is_current {
                    (DARK_ACCENT, Color32::WHITE, Stroke::NONE)
                } else {
                    (CARD_BG, DARK_TEXT_PRIMARY, Stroke::new(1.0, CARD_BORDER))
                };

                let display_name = if is_current && store.dirty {
                    format!("*{}", truncate_name(pname, 9))
                } else {
                    truncate_name(pname, 10)
                };
                let btn = egui::Button::new(
                    RichText::new(&display_name).size(SMALL_SIZE).color(text_color),
                )
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(4));

                let response = ui.add_sized(Vec2::new(btn_width, btn_height), btn);

                // Left click: load/reload preset (also clears pending delete)
                if response.clicked() {
                    new_pending = None;
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("pending_preset"), i));
                }

                // Right click: deselect current, or two-stage delete on others
                if response.secondary_clicked() {
                    if is_armed {
                        // Second right-click: confirm delete
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(egui::Id::new("delete_preset"), i));
                        new_pending = None;
                    } else if is_current && !store.dirty {
                        // Right-click current (clean) preset: deselect
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(egui::Id::new("deselect_preset"), true));
                        new_pending = None;
                    } else {
                        // First right-click: arm for delete
                        new_pending = Some((i, now));
                    }
                }

                let hover_text = if is_armed {
                    "Right-click again to DELETE".to_string()
                } else if is_current && store.dirty {
                    format!("{pname} — click to reload, right-click to delete")
                } else if is_current {
                    format!("{pname} — click to reload, right-click to deselect")
                } else {
                    format!("{pname} (right-click to delete)")
                };
                response.on_hover_text(hover_text);
            }
        });
    }

    // Persist pending delete state
    ui.ctx().data_mut(|d| {
        if let Some(pd) = new_pending {
            d.insert_temp(egui::Id::new("pending_delete_preset"), pd);
        } else {
            d.remove_temp::<(usize, f64)>(egui::Id::new("pending_delete_preset"));
        }
    });

    // Request repaint while armed (for timeout expiry)
    if new_pending.is_some() {
        ui.ctx().request_repaint();
    }
}

fn truncate_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        name.to_string()
    } else {
        format!("{}\u{2026}", &name[..max_len - 1])
    }
}
