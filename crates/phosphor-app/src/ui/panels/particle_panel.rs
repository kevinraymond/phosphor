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
    pub is_compute_raster: bool,
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
    // Morph state
    pub has_morph: bool,
    pub morph_target_count: u32,
    pub morph_source_index: u32,
    pub morph_dest_index: u32,
    pub morph_progress: f32,
    pub morph_transitioning: bool,
    pub morph_transition_style: u32,
    pub morph_auto_cycle: u32, // 0=Off, 1=OnBeat, 2=Timed
    pub morph_hold_duration: f32,
    pub morph_target_labels: Vec<String>,
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
            feature_badge(ui, &format!("trails ({}pt)", info.trail_length), tc.accent);
        }
        if info.has_interaction {
            feature_badge(ui, "interaction", tc.accent);
        }
        if info.has_sprite {
            feature_badge(ui, "sprite", tc.accent);
        }
        if info.is_compute_raster {
            feature_badge(
                ui,
                "COMPUTE",
                egui::Color32::from_rgb(0x40, 0xC0, 0xC0),
            );
        }
    });

    // Image source section (shown only for image emitter effects)
    if info.has_image_source {
        // Source loading indicator
        if info.source_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!("Loading {}...", info.source_loading_name))
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                );
            });
        }

        // Video transport controls (only when video source is active)
        if info.source_type == "video" {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let play_label = if info.video_playing { "Pause" } else { "Play" };
                if ui
                    .add(
                        egui::Button::new(RichText::new(play_label).size(SMALL_SIZE))
                            .min_size(egui::vec2(0.0, 24.0)),
                    )
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("particle_video_playing"), !info.video_playing);
                    });
                }
                let loop_label = if info.video_looping {
                    "Loop: On"
                } else {
                    "Loop: Off"
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new(loop_label).size(SMALL_SIZE))
                            .min_size(egui::vec2(0.0, 24.0)),
                    )
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("particle_video_looping"), !info.video_looping);
                    });
                }
            });

            // Speed slider
            let mut speed = info.video_speed;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Speed")
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                );
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

    // Morph section
    if info.has_morph {
        let morph_purple = egui::Color32::from_rgb(0xC0, 0x80, 0xE0);
        let morph_active = egui::Color32::from_rgb(0x80, 0x50, 0xA0);
        let morph_dest = egui::Color32::from_rgb(0x60, 0x40, 0x80);

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            feature_badge(ui, "MORPH", morph_purple);
            if info.morph_transitioning {
                ui.label(
                    RichText::new(format!("{:.0}%", info.morph_progress * 100.0))
                        .size(SMALL_SIZE)
                        .color(tc.accent),
                );
            }
        });

        // Transition progress bar
        let bar_height = 3.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), bar_height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, tc.widget_bg);
        if info.morph_transitioning {
            let mut fill_rect = rect;
            fill_rect.set_right(rect.left() + rect.width() * info.morph_progress.min(1.0));
            painter.rect_filled(fill_rect, 2.0, morph_purple);
        }

        ui.add_space(4.0);

        // --- Target slot grid ---
        // Track which slot is selected for replacement (persisted across frames)
        let sel_id = egui::Id::new("morph_selected_slot");
        let selected_slot: Option<u32> = ui.ctx().data(|d| d.get_temp(sel_id));

        // 4 fixed slots in a row: click to morph, right-click or select to replace
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for i in 0..4u32 {
                let has_target = i < info.morph_target_count;
                let label = if has_target {
                    info.morph_target_labels
                        .get(i as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("{}", i))
                } else {
                    "empty".to_string()
                };
                let is_current = has_target && !info.morph_transitioning && i == info.morph_dest_index;
                let is_morphing_to = has_target && info.morph_transitioning && i == info.morph_dest_index;
                let is_selected = selected_slot == Some(i);

                let btn = if is_selected {
                    egui::Button::new(
                        RichText::new(&label).size(SMALL_SIZE).color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(0xA0, 0x60, 0x30))
                } else if is_current {
                    egui::Button::new(
                        RichText::new(&label).size(SMALL_SIZE).color(egui::Color32::WHITE),
                    )
                    .fill(morph_active)
                } else if is_morphing_to {
                    egui::Button::new(
                        RichText::new(&label).size(SMALL_SIZE).color(egui::Color32::WHITE),
                    )
                    .fill(morph_dest)
                } else if !has_target {
                    egui::Button::new(
                        RichText::new(&label).size(SMALL_SIZE).color(tc.text_secondary.gamma_multiply(0.5)),
                    )
                } else {
                    egui::Button::new(RichText::new(&label).size(SMALL_SIZE))
                };

                let resp = ui.add(btn.min_size(egui::vec2(0.0, 22.0)));

                if resp.clicked() && has_target {
                    if is_selected {
                        // Deselect
                        ui.ctx().data_mut(|d| d.remove_temp::<u32>(sel_id));
                    } else {
                        // Morph to this target
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("morph_trigger_target"), i);
                        });
                    }
                }
                if resp.secondary_clicked() {
                    // Toggle select for replacement
                    if is_selected {
                        ui.ctx().data_mut(|d| d.remove_temp::<u32>(sel_id));
                    } else {
                        ui.ctx().data_mut(|d| d.insert_temp(sel_id, i));
                    }
                }
                if !has_target && resp.clicked() {
                    // Select empty slot
                    ui.ctx().data_mut(|d| d.insert_temp(sel_id, i));
                }
            }
        });

        // Show slot action hint
        if let Some(slot) = selected_slot {
            let slot_label = if slot < info.morph_target_count {
                info.morph_target_labels
                    .get(slot as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("{}", slot))
            } else {
                "empty".to_string()
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Slot {} [{}]", slot, slot_label))
                        .size(SMALL_SIZE)
                        .color(egui::Color32::from_rgb(0xA0, 0x60, 0x30)),
                );
                if slot < info.morph_target_count {
                    if ui
                        .add(egui::Button::new(
                            RichText::new("Clear").size(SMALL_SIZE - 1.0),
                        ).min_size(egui::vec2(0.0, 18.0)))
                        .clicked()
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("morph_clear_slot"), slot);
                            d.remove_temp::<u32>(sel_id);
                        });
                    }
                }
            });
        }

        // --- Manual blend slider ---
        if info.morph_target_count >= 2 {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // Source/dest selectors
                let src_label = info.morph_target_labels
                    .get(info.morph_source_index as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("{}", info.morph_source_index));
                let dst_label = info.morph_target_labels
                    .get(info.morph_dest_index as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("{}", info.morph_dest_index));

                // Source picker
                let mut src = info.morph_source_index;
                egui::ComboBox::from_id_salt("morph_blend_src")
                    .selected_text(&src_label)
                    .width(50.0)
                    .show_ui(ui, |ui| {
                        for i in 0..info.morph_target_count {
                            let label = info.morph_target_labels
                                .get(i as usize)
                                .cloned()
                                .unwrap_or_else(|| format!("{}", i));
                            if ui.selectable_label(src == i, &label).clicked() {
                                src = i;
                            }
                        }
                    });

                // Progress slider
                let mut progress = info.morph_progress;
                let r = ui.add(
                    egui::Slider::new(&mut progress, 0.0..=1.0)
                        .show_value(false)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                );

                // Dest picker
                let mut dst = info.morph_dest_index;
                egui::ComboBox::from_id_salt("morph_blend_dst")
                    .selected_text(&dst_label)
                    .width(50.0)
                    .show_ui(ui, |ui| {
                        for i in 0..info.morph_target_count {
                            let label = info.morph_target_labels
                                .get(i as usize)
                                .cloned()
                                .unwrap_or_else(|| format!("{}", i));
                            if ui.selectable_label(dst == i, &label).clicked() {
                                dst = i;
                            }
                        }
                    });

                // Emit changes
                if r.dragged() || r.changed() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("morph_manual_blend"), progress);
                    });
                }
                if src != info.morph_source_index {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("morph_set_source"), src);
                    });
                }
                if dst != info.morph_dest_index {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("morph_trigger_target"), dst);
                    });
                }
            });
        }

        ui.add_space(2.0);

        // --- Add targets row ---
        // The selected slot (or next empty, or last) is the destination
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if ui
                .add(
                    egui::Button::new(RichText::new("Image").size(SMALL_SIZE - 1.0))
                        .min_size(egui::vec2(0.0, 20.0)),
                )
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("morph_load_image"), true);
                });
            }
            // Geometry dropdown
            let geo_shapes = ["circle", "ring", "grid", "spiral", "heart", "star"];
            let mut selected_geo = String::new();
            egui::ComboBox::from_id_salt("morph_add_geometry")
                .selected_text("Shape")
                .width(62.0)
                .show_ui(ui, |ui| {
                    for shape in &geo_shapes {
                        if ui.selectable_label(false, *shape).clicked() {
                            selected_geo = shape.to_string();
                        }
                    }
                });
            if !selected_geo.is_empty() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("morph_add_geometry"), selected_geo);
                });
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("Snap").size(SMALL_SIZE - 1.0))
                        .min_size(egui::vec2(0.0, 20.0)),
                )
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("morph_snapshot"), true);
                });
            }
            #[cfg(feature = "video")]
            if ui
                .add(
                    egui::Button::new(RichText::new("Video").size(SMALL_SIZE - 1.0))
                        .min_size(egui::vec2(0.0, 20.0)),
                )
                .clicked()
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("morph_load_video"), true);
                });
            }
        });

        // Text input
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let text_id = egui::Id::new("morph_text_input_buf");
            let mut text_buf: String = ui.ctx().data_mut(|d| {
                d.get_persisted_mut_or_default::<String>(text_id).clone()
            });
            let te = ui.add(
                egui::TextEdit::singleline(&mut text_buf)
                    .desired_width(ui.available_width() - 42.0)
                    .hint_text("Text...")
                    .font(egui::TextStyle::Small),
            );
            if te.changed() {
                let buf = text_buf.clone();
                ui.ctx().data_mut(|d| {
                    *d.get_persisted_mut_or_default::<String>(text_id) = buf;
                });
            }
            let enter_pressed = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add_clicked = ui
                .add(
                    egui::Button::new(RichText::new("Add").size(SMALL_SIZE - 1.0))
                        .min_size(egui::vec2(0.0, 20.0)),
                )
                .clicked();
            if (add_clicked || enter_pressed) && !text_buf.is_empty() {
                let text = text_buf.clone();
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("morph_add_text"), text);
                    *d.get_persisted_mut_or_default::<String>(text_id) = String::new();
                });
            }
        });

        ui.add_space(2.0);

        // --- Settings (compact) ---
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // Style
            let style_labels = ["Spring", "Explode", "Flow", "Cascade", "Direct"];
            let mut style = info.morph_transition_style;
            egui::ComboBox::from_id_salt("morph_style")
                .selected_text(style_labels[style.min(4) as usize])
                .width(70.0)
                .show_ui(ui, |ui| {
                    for (i, label) in style_labels.iter().enumerate() {
                        if ui.selectable_label(style == i as u32, *label).clicked() {
                            style = i as u32;
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("morph_style"), style);
                            });
                        }
                    }
                });
            // Cycle
            let cycle_labels = ["Off", "Beat", "Timed"];
            let mut cycle_mode = info.morph_auto_cycle;
            egui::ComboBox::from_id_salt("morph_auto_cycle")
                .selected_text(cycle_labels[cycle_mode.min(2) as usize])
                .width(55.0)
                .show_ui(ui, |ui| {
                    for (i, label) in cycle_labels.iter().enumerate() {
                        if ui.selectable_label(cycle_mode == i as u32, *label).clicked() {
                            cycle_mode = i as u32;
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("morph_auto_cycle"), cycle_mode);
                            });
                        }
                    }
                });
            // Hold
            let mut hold = info.morph_hold_duration;
            let r = ui.add(
                egui::DragValue::new(&mut hold)
                    .range(0.0..=8.0)
                    .speed(0.05)
                    .suffix("s")
                    .custom_formatter(|v, _| format!("{:.1}s", v)),
            );
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("morph_hold_duration"), hold);
                });
            }
            r.on_hover_text("Hold duration");
        });

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
    let emit_max = emit_rate.max(info.max_count as f32 * 0.1).max(5000.0);
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
            ui.label(
                RichText::new("Emit rate")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
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

            ui.label(
                RichText::new("Burst")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
            let r = ui.add(egui::Slider::new(&mut burst, 0..=burst_max).show_value(true));
            if r.changed() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("particle_burst"), burst);
                });
            }
            ui.end_row();

            ui.label(
                RichText::new("Lifetime")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
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

            ui.label(
                RichText::new("Speed")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
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

            ui.label(
                RichText::new("Size")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
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

            ui.label(
                RichText::new("Drag")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
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
    egui::Frame::NONE
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
