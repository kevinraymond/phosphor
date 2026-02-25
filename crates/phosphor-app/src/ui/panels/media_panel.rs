use egui::{RichText, Ui};

use crate::media::types::PlayDirection;
use crate::ui::theme::tokens::*;

/// Info about the active media layer, collected before UI borrow.
pub struct MediaInfo {
    pub file_name: String,
    pub media_width: u32,
    pub media_height: u32,
    pub frame_count: usize,
    pub is_animated: bool,
    pub playing: bool,
    pub looping: bool,
    pub speed: f32,
    pub direction: PlayDirection,
    pub current_frame: usize,
}

pub fn draw_media_panel(ui: &mut Ui, info: &MediaInfo) {
    // File info
    ui.label(
        RichText::new(&info.file_name)
            .size(BODY_SIZE)
            .color(DARK_TEXT_PRIMARY),
    );
    ui.label(
        RichText::new(format!("{}x{}", info.media_width, info.media_height))
            .size(SMALL_SIZE)
            .color(DARK_TEXT_SECONDARY),
    );

    if info.is_animated {
        ui.label(
            RichText::new(format!(
                "Frame {}/{}",
                info.current_frame + 1,
                info.frame_count
            ))
            .size(SMALL_SIZE)
            .color(DARK_TEXT_SECONDARY),
        );

        ui.add_space(4.0);

        // Play/Pause
        ui.horizontal(|ui| {
            let play_label = if info.playing { "Pause" } else { "Play" };
            if ui
                .button(RichText::new(play_label).size(SMALL_SIZE))
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("media_play_pause"), true);
                });
            }

            // Loop toggle
            let mut looping = info.looping;
            if ui.checkbox(&mut looping, "Loop").changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("media_loop"), looping);
                });
            }
        });

        // Speed slider
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Speed")
                    .size(SMALL_SIZE)
                    .color(DARK_TEXT_SECONDARY),
            );
            let mut speed = info.speed;
            let slider = ui.add(
                egui::Slider::new(&mut speed, 0.1..=4.0)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.1}x", v))
                    .text(""),
            );
            if slider.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("media_speed"), speed);
                });
            }
        });

        // Direction
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Direction")
                    .size(SMALL_SIZE)
                    .color(DARK_TEXT_SECONDARY),
            );
            let directions: [(PlayDirection, u8, &str); 3] = [
                (PlayDirection::Forward, 0, "Fwd"),
                (PlayDirection::Reverse, 1, "Rev"),
                (PlayDirection::PingPong, 2, "Ping-Pong"),
            ];
            for (dir, dir_u8, label) in &directions {
                let selected = info.direction == *dir;
                if ui
                    .selectable_label(selected, RichText::new(*label).size(SMALL_SIZE))
                    .clicked()
                    && !selected
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("media_direction"), *dir_u8);
                    });
                }
            }
        });
    } else {
        ui.label(
            RichText::new("Static image")
                .size(SMALL_SIZE)
                .color(DARK_TEXT_SECONDARY),
        );
    }
}
