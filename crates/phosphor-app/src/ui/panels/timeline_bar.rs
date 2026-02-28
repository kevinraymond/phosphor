use egui::{Color32, CornerRadius, Rect, StrokeKind, Ui, Vec2};

use crate::scene::timeline::{TimelineInfo, TimelineInfoState};
use crate::scene::types::TransitionType;
use crate::ui::theme::colors::theme_colors;
use crate::ui::theme::tokens::*;

/// Draw a horizontal timeline bar showing cue blocks and playhead.
/// Returns true if the timeline bar is visible and was drawn.
pub fn draw_timeline_bar(
    ui: &mut Ui,
    timeline: &TimelineInfo,
    cue_names: &[String],
) -> bool {
    if !timeline.active || timeline.cue_count == 0 {
        return false;
    }

    let tc = theme_colors(ui.ctx());
    let available_width = ui.available_width();
    let bar_height = 32.0;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let block_width = if timeline.cue_count > 0 {
            available_width / timeline.cue_count as f32
        } else {
            available_width
        };

        for i in 0..timeline.cue_count {
            let is_current = i == timeline.current_cue;
            let is_transitioning_to = matches!(
                &timeline.state,
                TimelineInfoState::Transitioning { to, .. } if *to == i
            );
            let is_transitioning_from = matches!(
                &timeline.state,
                TimelineInfoState::Transitioning { from, .. } if *from == i
            );

            // Background color
            let bg = if is_current {
                tc.accent.linear_multiply(0.3)
            } else if is_transitioning_from || is_transitioning_to {
                tc.accent.linear_multiply(0.15)
            } else {
                tc.card_bg
            };

            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(block_width, bar_height),
                egui::Sense::click(),
            );

            // Draw block background
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(2), bg);

            // Draw border
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                egui::Stroke::new(1.0, tc.card_border),
                StrokeKind::Outside,
            );

            // Transition progress overlay
            if is_transitioning_to {
                if let TimelineInfoState::Transitioning { progress, transition_type, .. } = &timeline.state {
                    let overlay_color = match transition_type {
                        TransitionType::Cut => Color32::TRANSPARENT,
                        TransitionType::Dissolve => tc.accent.linear_multiply(0.2 * progress),
                        TransitionType::ParamMorph => Color32::from_rgba_unmultiplied(
                            tc.success.r(), tc.success.g(), tc.success.b(),
                            (40.0 * progress) as u8,
                        ),
                    };
                    let progress_rect = Rect::from_min_size(
                        rect.min,
                        Vec2::new(rect.width() * progress, rect.height()),
                    );
                    painter.rect_filled(progress_rect, CornerRadius::same(2), overlay_color);
                }
            }

            // Cue label
            let cue_label = cue_names.get(i).map(|s| s.as_str()).unwrap_or("?");
            let label_color = if is_current { tc.text_primary } else { tc.text_secondary };
            let galley = painter.layout_no_wrap(
                format!("{}", cue_label),
                egui::FontId::proportional(SMALL_SIZE),
                label_color,
            );
            let text_pos = egui::pos2(
                rect.center().x - galley.size().x * 0.5,
                rect.center().y - galley.size().y * 0.5,
            );
            painter.galley(text_pos, galley, label_color);

            // Click to jump
            if response.clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("scene_jump_to_cue"), i);
                });
            }
        }

        // Playhead indicator
        let base_y = ui.min_rect().min.y;
        match &timeline.state {
            TimelineInfoState::Transitioning { to, progress, transition_type, .. } => {
                // Show playhead as progress within the target cue block
                let cue_x = *to as f32 * block_width;
                let playhead_x = cue_x + progress * block_width;
                let top = egui::pos2(playhead_x, base_y);
                let bottom = egui::pos2(playhead_x, base_y + bar_height);
                let playhead_color = match transition_type {
                    TransitionType::Dissolve => tc.accent,
                    TransitionType::ParamMorph => tc.success,
                    TransitionType::Cut => tc.accent,
                };
                ui.painter().line_segment(
                    [top, bottom],
                    egui::Stroke::new(2.0, playhead_color),
                );

                // Transition label centered in the bar
                let label = format!(
                    "{} {:.0}%",
                    transition_type.display_name(),
                    progress * 100.0,
                );
                let galley = ui.painter().layout_no_wrap(
                    label,
                    egui::FontId::proportional(SMALL_SIZE),
                    tc.text_primary,
                );
                let label_x = available_width * 0.5 - galley.size().x * 0.5;
                let label_y = base_y + bar_height - galley.size().y - 2.0;
                ui.painter().galley(
                    egui::pos2(label_x, label_y),
                    galley,
                    tc.text_primary,
                );
            }
            TimelineInfoState::Holding { elapsed, hold_secs } => {
                // Playhead within the current cue block
                let cue_x = timeline.current_cue as f32 * block_width;
                let playhead_x = if let Some(hold) = hold_secs {
                    let frac = (elapsed / hold).min(1.0);
                    cue_x + frac * block_width
                } else {
                    cue_x + block_width * 0.5
                };
                let top = egui::pos2(playhead_x, base_y);
                let bottom = egui::pos2(playhead_x, base_y + bar_height);
                ui.painter().line_segment(
                    [top, bottom],
                    egui::Stroke::new(2.0, tc.accent),
                );
            }
            _ => {}
        }
    });

    true
}
