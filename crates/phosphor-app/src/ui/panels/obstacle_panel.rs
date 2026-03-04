use egui::{RichText, Ui};

use crate::gpu::particle::ObstacleMode;
use crate::ui::theme::colors::theme_colors;
use crate::ui::theme::tokens::*;

/// Snapshot of obstacle state, collected before UI borrow to avoid borrow conflicts.
#[derive(Clone)]
pub struct ObstacleInfo {
    pub enabled: bool,
    pub mode: ObstacleMode,
    pub threshold: f32,
    pub elasticity: f32,
    /// "image", "webcam", "depth", or "" (none)
    pub source: String,
    pub image_path: Option<String>,
    pub has_particles: bool,
    pub webcam_available: bool,
    pub video_available: bool,
    pub depth_available: bool,
    pub depth_model_downloaded: bool,
    /// Download progress percentage (0-100), or None if not downloading.
    pub depth_downloading: Option<u8>,
    pub depth_download_error: Option<String>,
}

/// UI commands emitted by the obstacle panel.
#[derive(Clone, Default)]
pub enum ObstacleCommand {
    #[default]
    None,
    SetEnabled(bool),
    SetMode(ObstacleMode),
    SetThreshold(f32),
    SetElasticity(f32),
    LoadImage,
    LoadVideo,
    UseWebcam,
    UseDepth,
    DownloadDepthModel,
    Clear,
}

pub fn draw_obstacle_panel(ui: &mut Ui, info: &ObstacleInfo) {
    let tc = theme_colors(ui.ctx());

    if !info.has_particles {
        ui.label(
            RichText::new("No particle system active")
                .size(BODY_SIZE)
                .color(tc.text_secondary),
        );
        return;
    }

    // Enable toggle
    let mut enabled = info.enabled;
    if ui
        .checkbox(&mut enabled, "Enable Obstacle")
        .on_hover_text("Enable particle-obstacle collision")
        .changed()
    {
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new("obstacle_cmd"),
                ObstacleCommand::SetEnabled(enabled),
            )
        });
    }

    if !info.enabled {
        return;
    }

    ui.add_space(4.0);

    // Source info + controls
    ui.horizontal(|ui| {
        let source_text = match info.source.as_str() {
            "image" => {
                if let Some(ref path) = info.image_path {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "image".to_string());
                    format!("Image: {}", name)
                } else {
                    "Image".to_string()
                }
            }
            "video" => {
                if let Some(ref path) = info.image_path {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "video".to_string());
                    format!("Video: {}", name)
                } else {
                    "Video".to_string()
                }
            }
            "webcam" => "Webcam".to_string(),
            "depth" => "Depth (MiDaS)".to_string(),
            _ => "None".to_string(),
        };
        ui.label(
            RichText::new(source_text)
                .size(BODY_SIZE)
                .color(tc.text_primary),
        );
    });

    ui.horizontal(|ui| {
        if ui
            .button("Image")
            .on_hover_text("Load an image as obstacle shape")
            .clicked()
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("obstacle_cmd"), ObstacleCommand::LoadImage)
            });
        }
        if info.video_available {
            if ui
                .button("Video")
                .on_hover_text("Load a video as animated obstacle")
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("obstacle_cmd"), ObstacleCommand::LoadVideo)
                });
            }
        }
        if info.webcam_available && info.source != "webcam" {
            if ui
                .button("Webcam")
                .on_hover_text("Use live webcam feed as obstacle")
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("obstacle_cmd"), ObstacleCommand::UseWebcam)
                });
            }
        }
        // Depth button: requires webcam + depth feature
        if info.depth_available && info.webcam_available {
            if info.depth_model_downloaded {
                if info.source != "depth" {
                    if ui
                        .button("Depth")
                        .on_hover_text("Monocular depth estimation (MiDaS)")
                        .clicked()
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("obstacle_cmd"), ObstacleCommand::UseDepth)
                        });
                    }
                }
            } else if info.depth_downloading.is_some() {
                let pct = info.depth_downloading.unwrap_or(0);
                ui.add_enabled(false, egui::Button::new(format!("Depth {pct}%")));
            } else {
                // Show "Depth" button that opens confirmation modal
                if ui
                    .button("Depth")
                    .on_hover_text("Requires one-time download (~80 MB)")
                    .clicked()
                {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("depth_download_confirm"), true));
                }
            }
        }
        if !info.source.is_empty() {
            if ui
                .button("Clear")
                .on_hover_text("Remove obstacle and stop capture")
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("obstacle_cmd"), ObstacleCommand::Clear)
                });
            }
        }
    });

    // Show download error if any
    if let Some(ref err) = info.depth_download_error {
        ui.label(
            RichText::new(format!("Download error: {err}"))
                .size(BODY_SIZE - 1.0)
                .color(tc.error),
        );
    }

    ui.add_space(4.0);

    // Mode dropdown
    let mut mode = info.mode;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Mode")
                .size(BODY_SIZE)
                .color(tc.text_secondary),
        )
        .on_hover_text("How particles respond when hitting the obstacle");
        egui::ComboBox::from_id_salt("obstacle_mode")
            .selected_text(mode.label())
            .show_ui(ui, |ui| {
                for m in [
                    ObstacleMode::Bounce,
                    ObstacleMode::Stick,
                    ObstacleMode::Flow,
                    ObstacleMode::Contain,
                ] {
                    if ui.selectable_value(&mut mode, m, m.label()).changed() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("obstacle_cmd"),
                                ObstacleCommand::SetMode(mode),
                            )
                        });
                    }
                }
            });
    });

    // Threshold slider
    let mut threshold = info.threshold;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Threshold")
                .size(BODY_SIZE)
                .color(tc.text_secondary),
        )
        .on_hover_text("Alpha cutoff for collision detection (lower = more sensitive)");
        if ui
            .add(egui::Slider::new(&mut threshold, 0.0..=1.0).step_by(0.01))
            .changed()
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("obstacle_cmd"),
                    ObstacleCommand::SetThreshold(threshold),
                )
            });
        }
    });

    // Elasticity slider
    let mut elasticity = info.elasticity;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Elasticity")
                .size(BODY_SIZE)
                .color(tc.text_secondary),
        )
        .on_hover_text("Energy preserved on bounce (0 = absorb, 1 = full)");
        if ui
            .add(egui::Slider::new(&mut elasticity, 0.0..=1.0).step_by(0.01))
            .changed()
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("obstacle_cmd"),
                    ObstacleCommand::SetElasticity(elasticity),
                )
            });
        }
    });
}

/// Draw the depth download confirmation modal (must be called at top level, not inside a panel).
pub fn draw_depth_download_modal(ctx: &egui::Context) {
    let show: bool = ctx.data(|d| {
        d.get_temp(egui::Id::new("depth_download_confirm"))
            .unwrap_or(false)
    });
    if !show {
        return;
    }

    let tc = theme_colors(ctx);

    egui::Window::new("Download Depth Model")
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::Vec2::new(340.0, 0.0))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Depth-based obstacle collision uses monocular depth estimation to create a 3D collision map from your webcam.")
                    .size(13.0)
                    .color(tc.text_primary),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("This requires a one-time download:")
                    .size(13.0)
                    .color(tc.text_secondary),
            );
            ui.add_space(4.0);
            ui.indent("dl_details", |ui| {
                ui.label(RichText::new("ONNX Runtime (~15 MB)").size(12.0).color(tc.text_secondary));
                ui.label(RichText::new("  from github.com/microsoft").size(11.0).color(tc.text_secondary));
                ui.label(RichText::new("MiDaS v2.1 model (~63 MB)").size(12.0).color(tc.text_secondary));
                ui.label(RichText::new("  from huggingface.co").size(11.0).color(tc.text_secondary));
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Files are cached locally and only downloaded once.")
                    .size(12.0)
                    .color(tc.text_secondary),
            );
            ui.add_space(12.0);

            let btn_size = egui::Vec2::new(110.0, 32.0);
            ui.horizontal(|ui| {
                let accent = tc.accent;
                let dl_fill = egui::Color32::from_rgba_unmultiplied(
                    accent.r(), accent.g(), accent.b(), 60,
                );
                if ui
                    .add(egui::Button::new(
                        RichText::new("Download").color(accent),
                    ).fill(dl_fill).min_size(btn_size))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("depth_download_confirm"), false);
                        d.insert_temp(egui::Id::new("obstacle_cmd"), ObstacleCommand::DownloadDepthModel);
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("Cancel").min_size(btn_size)).clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("depth_download_confirm"), false);
                        });
                    }
                });
            });
        });
}
