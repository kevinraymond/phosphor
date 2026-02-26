use egui::{Color32, CornerRadius, RichText, Stroke, Ui, Vec2};

use crate::effect::EffectLoader;
use crate::ui::theme::colors::theme_colors;
use crate::ui::theme::tokens::*;

const COLS: usize = 3;

pub fn draw_effect_panel(ui: &mut Ui, loader: &EffectLoader) {
    let tc = theme_colors(ui.ctx());

    if loader.effects.is_empty() {
        ui.label(RichText::new("No effects found").size(SMALL_SIZE).color(tc.text_secondary));
        return;
    }

    let available_width = ui.available_width();
    let gap = 4.0;
    let total_gaps = (COLS - 1) as f32 * gap;
    let btn_width = ((available_width - total_gaps) / COLS as f32).max(40.0);
    let btn_height = 22.0;

    let effects: Vec<_> = loader.effects.iter().enumerate().collect();
    for row in effects.chunks(COLS) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(i, effect) in row {
                let is_current = loader.current_effect == Some(i);

                let (fill, text_color, stroke) = if is_current {
                    (tc.accent, Color32::WHITE, Stroke::NONE)
                } else {
                    (tc.card_bg, tc.text_primary, Stroke::new(1.0, tc.card_border))
                };

                let btn = egui::Button::new(
                    RichText::new(truncate_name(&effect.name, 10))
                        .size(SMALL_SIZE)
                        .color(text_color),
                )
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(4));

                let response = ui.add_sized(Vec2::new(btn_width, btn_height), btn);
                if response.clicked() && !is_current {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("pending_effect"), i));
                }
                if !effect.description.is_empty() {
                    response.on_hover_text(&effect.description);
                }
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
