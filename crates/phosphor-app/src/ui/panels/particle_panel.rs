use egui::{RichText, Ui};

use crate::ui::theme::colors::theme_colors;
use crate::ui::theme::tokens::*;

/// Snapshot of particle system state, collected before UI borrow.
#[derive(Clone)]
pub struct ParticleInfo {
    pub alive_count: u32,
    pub max_count: u32,
    pub emit_rate: f32,
    pub burst_on_beat: u32,
    pub lifetime: f32,
    pub initial_speed: f32,
    pub initial_size: f32,
    pub size_end: f32,
    pub drag: f32,
    pub attraction_strength: f32,
    pub blend_mode: String,
    pub has_flow_field: bool,
    pub has_trails: bool,
    pub trail_length: u32,
    pub has_interaction: bool,
    pub has_sprite: bool,
    // Image source info
    pub has_image_source: bool,
    /// "static", "video", or "webcam"
    pub source_type: String,
    /// Source filename or device name
    pub source_name: String,
    pub video_playing: bool,
    pub video_looping: bool,
    pub video_speed: f32,
    pub video_position_secs: f64,
    pub video_duration_secs: f64,
    pub is_transitioning: bool,
    pub source_loading: bool,
    pub source_loading_name: String,
    /// Built-in image names (e.g. "skull", "phoenix") available for quick select.
    pub builtin_images: Vec<String>,
}

pub fn draw_particle_panel(ui: &mut Ui, info: &ParticleInfo) {
    let tc = theme_colors(ui.ctx());

    // Alive / max count with utilization bar
    let util = if info.max_count > 0 {
        info.alive_count as f32 / info.max_count as f32
    } else {
        0.0
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} / {} alive ({:.0}%)",
                format_count(info.alive_count),
                format_count(info.max_count),
                util * 100.0,
            ))
            .size(BODY_SIZE)
            .color(tc.text_primary),
        );
    });

    // Utilization bar
    let bar_height = 4.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), bar_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, tc.widget_bg);
    let mut fill_rect = rect;
    fill_rect.set_right(rect.left() + rect.width() * util.min(1.0));
    let bar_color = if util > 0.9 {
        egui::Color32::from_rgb(0xC0, 0x40, 0x40) // Red when near capacity
    } else if util > 0.7 {
        egui::Color32::from_rgb(0xC0, 0x90, 0x30) // Yellow
    } else {
        egui::Color32::from_rgb(0x40, 0x90, 0x60) // Green
    };
    painter.rect_filled(fill_rect, 2.0, bar_color);

    ui.add_space(6.0);

    // Feature badges
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        feature_badge(ui, &info.blend_mode, tc.text_secondary);
        if info.has_flow_field {
            feature_badge(ui, "flow field", tc.accent);
        }
        if info.has_trails {
            feature_badge(
                ui,
                &format!("trails ({}pt)", info.trail_length),
                tc.accent,
            );
        }
        if info.has_interaction {
            feature_badge(ui, "interaction", tc.accent);
        }
        if info.has_sprite {
            feature_badge(ui, "sprite", tc.accent);
        }
    });

    // Image source section (shown only for image emitter effects)
    if info.has_image_source {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // Source type badge
            let badge_text = match info.source_type.as_str() {
                "video" => "VIDEO",
                "webcam" => "CAM",
                _ => "IMG",
            };
            let badge_color = match info.source_type.as_str() {
                "video" => egui::Color32::from_rgb(0x60, 0x80, 0xC0),
                "webcam" => egui::Color32::from_rgb(0xC0, 0x60, 0x60),
                _ => tc.text_secondary,
            };
            feature_badge(ui, badge_text, badge_color);
            if !info.source_name.is_empty() {
                let name = if info.source_name.len() > 20 {
                    format!("{}...", &info.source_name[..17])
                } else {
                    info.source_name.clone()
                };
                let label = ui.label(
                    RichText::new(&name)
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                );
                if info.source_name.len() > 20 {
                    label.on_hover_text(&info.source_name);
                }
            }
            if info.is_transitioning {
                ui.label(
                    RichText::new("transitioning...")
                        .size(SMALL_SIZE - 1.0)
                        .color(tc.accent),
                );
            }
        });

        // Built-in image selector
        if !info.builtin_images.is_empty() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Image")
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                );
                // Derive current selection label from source_name
                let current_label = if info.source_type == "static" && !info.source_name.is_empty() {
                    // Strip "raster_" prefix and ".png" suffix for display
                    let name = info.source_name.trim_end_matches(".png");
                    let name = name.strip_prefix("raster_").unwrap_or(name);
                    name.to_string()
                } else {
                    "—".to_string()
                };
                egui::ComboBox::from_id_salt("particle_builtin_image")
                    .selected_text(&current_label)
                    .width(ui.available_width() - 4.0)
                    .show_ui(ui, |ui| {
                        for name in &info.builtin_images {
                            if ui
                                .selectable_label(*name == current_label, name)
                                .clicked()
                            {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("particle_select_builtin"),
                                        name.clone(),
                                    );
                                });
                            }
                        }
                    });
            });
            ui.add_space(2.0);
        }

        // Source action buttons
        if info.source_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!("Loading {}...", info.source_loading_name))
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                );
            });
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui
                    .add(egui::Button::new(RichText::new("Load Image").size(SMALL_SIZE)).min_size(egui::vec2(0.0, 24.0)))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("particle_load_image"), true);
                    });
                }
                #[cfg(feature = "video")]
                if ui
                    .add(egui::Button::new(RichText::new("Load Video").size(SMALL_SIZE)).min_size(egui::vec2(0.0, 24.0)))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("particle_load_video"), true);
                    });
                }
                #[cfg(feature = "webcam")]
                if ui
                    .add(egui::Button::new(RichText::new("Webcam").size(SMALL_SIZE)).min_size(egui::vec2(0.0, 24.0)))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("particle_webcam"), true);
                    });
                }
            });
        }

        // Video transport controls (only when video source is active)
        if info.source_type == "video" {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let play_label = if info.video_playing { "Pause" } else { "Play" };
                if ui
                    .add(egui::Button::new(RichText::new(play_label).size(SMALL_SIZE)).min_size(egui::vec2(0.0, 24.0)))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("particle_video_playing"),
                            !info.video_playing,
                        );
                    });
                }
                let loop_label = if info.video_looping { "Loop: On" } else { "Loop: Off" };
                if ui
                    .add(egui::Button::new(RichText::new(loop_label).size(SMALL_SIZE)).min_size(egui::vec2(0.0, 24.0)))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("particle_video_looping"),
                            !info.video_looping,
                        );
                    });
                }
            });

            // Speed slider
            let mut speed = info.video_speed;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Speed").size(SMALL_SIZE).color(tc.text_secondary));
                let r = ui.add(
                    egui::Slider::new(&mut speed, 0.1..=4.0)
                        .show_value(true)
                        .custom_formatter(|v, _| format!("{:.1}x", v)),
                );
                if r.changed() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("particle_video_speed"), speed);
                    });
                }
            });

            // Seek bar
            if info.video_duration_secs > 0.0 {
                let mut pos = info.video_position_secs as f32;
                let dur = info.video_duration_secs as f32;
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::Slider::new(&mut pos, 0.0..=dur)
                            .show_value(false)
                            .custom_formatter(|v, _| {
                                let s = v as u32;
                                format!("{}:{:02}", s / 60, s % 60)
                            }),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{:.0}:{:02.0} / {:.0}:{:02.0}",
                            (info.video_position_secs as u32) / 60,
                            (info.video_position_secs as u32) % 60,
                            (info.video_duration_secs as u32) / 60,
                            (info.video_duration_secs as u32) % 60,
                        ))
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                    );
                    if r.changed() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("particle_video_seek"), pos as f64);
                        });
                    }
                });
            }
        }
        ui.add_space(4.0);
    }

    ui.add_space(6.0);

    // Editable params (communicated back via egui temp data)
    let mut emit_rate = info.emit_rate;
    let mut burst = info.burst_on_beat;
    let mut lifetime = info.lifetime;
    let mut speed = info.initial_speed;
    let mut size = info.initial_size;
    let mut drag = info.drag;

    // Dynamic ranges: extend to include current value so out-of-range
    // values (e.g. Raster's emit_rate=100K, lifetime=999) aren't silently
    // clamped by the slider, which would corrupt ps.def every frame.
    let emit_max = emit_rate.max(5000.0);
    let burst_max = burst.max(2000);
    let life_max = lifetime.max(30.0);
    let speed_min = speed.min(0.0);
    let speed_max = speed.max(2.0);
    let speed_log = speed > 0.0;
    let size_min = size.min(0.001);
    let size_max = size.max(0.1);
    let drag_min = drag.min(0.8);

    egui::Grid::new("particle_params")
        .num_columns(2)
        .spacing([8.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Emit rate").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut emit_rate, 10.0..=emit_max)
                    .logarithmic(true)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.0}/s", v)),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_emit_rate"), emit_rate);
                });
            }
            ui.end_row();

            ui.label(RichText::new("Burst").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut burst, 0..=burst_max)
                    .show_value(true),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_burst"), burst);
                });
            }
            ui.end_row();

            ui.label(RichText::new("Lifetime").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut lifetime, 0.5..=life_max)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.1}s", v)),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_lifetime"), lifetime);
                });
            }
            ui.end_row();

            ui.label(RichText::new("Speed").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut speed, speed_min..=speed_max)
                    .logarithmic(speed_log)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.3}", v)),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_speed"), speed);
                });
            }
            ui.end_row();

            ui.label(RichText::new("Size").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut size, size_min..=size_max)
                    .logarithmic(true)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.4}", v)),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_size"), size);
                });
            }
            ui.end_row();

            ui.label(RichText::new("Drag").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut drag, drag_min..=1.0)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.3}", v)),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_drag"), drag);
                });
            }
            ui.end_row();
        });
}

fn feature_badge(ui: &mut Ui, text: &str, color: egui::Color32) {
    let tc = theme_colors(ui.ctx());
    egui::Frame::none()
        .fill(tc.widget_bg)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(SMALL_SIZE - 1.0).color(color));
        });
}

fn format_count(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f32 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f32 / 1_000.0)
    } else {
        format!("{}", n)
    }
}
