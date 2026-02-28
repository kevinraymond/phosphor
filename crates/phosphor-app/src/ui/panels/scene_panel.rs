use egui::{RichText, Ui};

use crate::scene::timeline::{TimelineInfo, TimelineInfoState};
use crate::scene::types::TransitionType;
use crate::ui::theme::colors::theme_colors;
use crate::ui::theme::tokens::*;

/// Info passed from App to the scene panel (avoids borrow conflicts).
#[derive(Debug, Clone)]
pub struct SceneInfo {
    pub scene_store_names: Vec<String>,
    pub current_scene: Option<usize>,
    pub timeline: Option<TimelineInfo>,
    pub preset_names: Vec<String>,
    /// Cues currently in the timeline (name, transition, duration).
    pub cue_list: Vec<CueDisplayInfo>,
}

#[derive(Debug, Clone)]
pub struct CueDisplayInfo {
    pub preset_name: String,
    pub transition: TransitionType,
    pub transition_secs: f32,
}

pub fn draw_scene_panel(ui: &mut Ui, info: &SceneInfo) {
    let tc = theme_colors(ui.ctx());

    // Scene select / save / delete
    let mut scene_save_name: String = ui
        .ctx()
        .data_mut(|d| d.get_temp(egui::Id::new("scene_save_name")).unwrap_or_default());

    // Save row
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut scene_save_name)
                .hint_text("Scene name...")
                .desired_width(140.0),
        );
        let can_save = !scene_save_name.trim().is_empty();
        let save_btn = ui.add_enabled(can_save, egui::Button::new(RichText::new("SAVE").size(SMALL_SIZE)));
        if save_btn.clicked()
            || (can_save
                && response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("save_scene"),
                    scene_save_name.trim().to_string(),
                );
            });
            scene_save_name.clear();
        }
    });
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new("scene_save_name"), scene_save_name);
    });

    // Scene list
    if info.scene_store_names.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new("No scenes saved")
                .size(SMALL_SIZE)
                .color(tc.text_secondary),
        );
    } else {
        ui.add_space(4.0);

        // Delete confirmation state
        let now = ui.input(|i| i.time);
        let pending_delete: Option<(usize, f64)> = ui
            .ctx()
            .data_mut(|d| d.get_temp(egui::Id::new("pending_delete_scene")));
        let pending_delete = pending_delete.filter(|(_, t)| now - t < 3.0);

        for (i, name) in info.scene_store_names.iter().enumerate() {
            let is_current = info.current_scene == Some(i);
            ui.horizontal(|ui| {
                let label = if is_current {
                    RichText::new(name).size(SMALL_SIZE).color(tc.accent).strong()
                } else {
                    RichText::new(name).size(SMALL_SIZE).color(tc.text_primary)
                };

                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("load_scene"), i);
                    });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_armed = pending_delete.map_or(false, |(idx, _)| idx == i);
                    let del_text = if is_armed { "Sure?" } else { "×" };
                    let del_color = if is_armed { tc.error } else { tc.text_secondary };
                    if ui
                        .add(egui::Button::new(RichText::new(del_text).size(SMALL_SIZE).color(del_color)).frame(false))
                        .clicked()
                    {
                        if is_armed {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("delete_scene"), i);
                                d.remove_temp::<(usize, f64)>(egui::Id::new("pending_delete_scene"));
                            });
                        } else {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("pending_delete_scene"), (i, now));
                            });
                        }
                    }
                });
            });
        }

        // Clear stale delete confirmation
        if let Some((_, t)) = pending_delete {
            if now - t >= 3.0 {
                ui.ctx().data_mut(|d| {
                    d.remove_temp::<(usize, f64)>(egui::Id::new("pending_delete_scene"));
                });
            }
        }
    }

    // Cue editing — show whenever a scene is selected
    if info.current_scene.is_some() {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // Transport controls (only when cues exist)
        if let Some(ref tl) = info.timeline {
            ui.horizontal(|ui| {
                let play_text = if tl.active { "Stop" } else { "Play" };
                if ui.button(RichText::new(play_text).size(SMALL_SIZE)).clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("scene_toggle_play"), true);
                    });
                }

                if tl.active {
                    if ui.button(RichText::new("Prev").size(SMALL_SIZE)).clicked() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("scene_go_prev"), true);
                        });
                    }
                    if ui.button(RichText::new("GO").size(SMALL_SIZE).strong()).clicked() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("scene_go_next"), true);
                        });
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut loop_mode = tl.loop_mode;
                    if ui.checkbox(&mut loop_mode, RichText::new("Loop").size(SMALL_SIZE)).changed() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("scene_set_loop"), loop_mode);
                        });
                    }
                });
            });

            ui.add_space(4.0);
        }

        // Cue list
        if info.cue_list.is_empty() {
            ui.label(
                RichText::new("No cues — add presets below")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
        } else {
            let current_cue = info.timeline.as_ref().map(|t| t.current_cue).unwrap_or(0);
            let active = info.timeline.as_ref().map(|t| t.active).unwrap_or(false);
            let transitioning_to = info.timeline.as_ref().and_then(|t| {
                if let TimelineInfoState::Transitioning { to, .. } = &t.state {
                    Some(*to)
                } else {
                    None
                }
            });

            for (idx, cue) in info.cue_list.iter().enumerate() {
                let is_current = active && idx == current_cue;
                let is_target = transitioning_to == Some(idx);
                let bg = if is_current {
                    tc.accent.linear_multiply(0.15)
                } else if is_target {
                    tc.accent.linear_multiply(0.08)
                } else {
                    egui::Color32::TRANSPARENT
                };

                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(egui::CornerRadius::same(3))
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Cue number
                            ui.label(
                                RichText::new(format!("{}.", idx + 1))
                                    .size(SMALL_SIZE)
                                    .color(tc.text_secondary),
                            );

                            // Click preset name to jump
                            let name_color = if is_current { tc.accent } else { tc.text_primary };
                            if ui
                                .add(egui::Button::new(
                                    RichText::new(&cue.preset_name)
                                        .size(SMALL_SIZE)
                                        .color(name_color),
                                ).frame(false))
                                .clicked()
                            {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(egui::Id::new("scene_jump_to_cue"), idx);
                                });
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Remove cue button
                                if ui
                                    .add(egui::Button::new(
                                        RichText::new("×").size(SMALL_SIZE).color(tc.text_secondary),
                                    ).frame(false))
                                    .clicked()
                                {
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(egui::Id::new("scene_remove_cue"), idx);
                                    });
                                }

                                // Duration (only for non-Cut)
                                if cue.transition != TransitionType::Cut {
                                    ui.label(
                                        RichText::new(format!("{:.1}s", cue.transition_secs))
                                            .size(SMALL_SIZE)
                                            .color(tc.text_secondary),
                                    );
                                }
                                // Transition type
                                ui.label(
                                    RichText::new(cue.transition.display_name())
                                        .size(SMALL_SIZE)
                                        .color(tc.text_secondary),
                                );
                            });
                        });
                    });
            }
        }

        // Add Cue from preset
        ui.add_space(4.0);
        if !info.preset_names.is_empty() {
            let mut add_cue_preset: usize = ui.ctx().data_mut(|d| {
                d.get_temp(egui::Id::new("add_cue_preset_idx")).unwrap_or(0)
            });

            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("add_cue_combo")
                    .width(140.0)
                    .selected_text(
                        info.preset_names
                            .get(add_cue_preset)
                            .map(|s| s.as_str())
                            .unwrap_or("Select preset"),
                    )
                    .show_ui(ui, |ui| {
                        for (i, name) in info.preset_names.iter().enumerate() {
                            ui.selectable_value(&mut add_cue_preset, i, name);
                        }
                    });

                if ui
                    .button(RichText::new("+ Cue").size(SMALL_SIZE))
                    .clicked()
                {
                    if let Some(name) = info.preset_names.get(add_cue_preset) {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("scene_add_cue"), name.clone());
                        });
                    }
                }
            });

            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("add_cue_preset_idx"), add_cue_preset);
            });
        } else {
            ui.label(
                RichText::new("Save some presets first")
                    .size(SMALL_SIZE)
                    .color(tc.text_secondary),
            );
        }
    }
}
