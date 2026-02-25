use egui::{Color32, CornerRadius, RichText, Stroke, Ui, Vec2};

use crate::preset::PresetStore;
use crate::ui::theme::tokens::*;

const COLS: usize = 3;

pub fn draw_preset_panel(ui: &mut Ui, store: &PresetStore) {
    // Compact save row
    let mut name = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(egui::Id::new("preset_save_name")))
        .unwrap_or_default();

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

    let available_width = ui.available_width();
    let gap = 4.0;
    let total_gaps = (COLS - 1) as f32 * gap;
    let btn_width = ((available_width - total_gaps) / COLS as f32).max(40.0);
    let btn_height = 22.0;

    let presets: Vec<_> = store.presets.iter().enumerate().collect();
    for row in presets.chunks(COLS) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(i, (pname, _)) in row {
                let is_current = store.current_preset == Some(i);

                let (fill, text_color, stroke) = if is_current {
                    (DARK_ACCENT, Color32::WHITE, Stroke::NONE)
                } else {
                    (CARD_BG, DARK_TEXT_PRIMARY, Stroke::new(1.0, CARD_BORDER))
                };

                let display = truncate_name(pname, 10);
                let btn = egui::Button::new(
                    RichText::new(&display).size(SMALL_SIZE).color(text_color),
                )
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(4));

                let response = ui.add_sized(Vec2::new(btn_width, btn_height), btn);
                if response.clicked() && !is_current {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("pending_preset"), i));
                }
                // Right-click to delete
                if response.secondary_clicked() {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("delete_preset"), i));
                }
                response.on_hover_text(format!("{pname} (right-click to delete)"));
            }
        });
    }
}

fn truncate_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        name.to_string()
    } else {
        format!("{}\u{2026}", &name[..max_len - 1])
    }
}
