use egui::{Color32, CornerRadius, RichText, Stroke, Ui, Vec2};

use crate::scene::timeline::{TimelineInfo, TimelineInfoState};
use crate::scene::types::{AdvanceMode, TransitionType};
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
    pub hold_secs: Option<f32>,
}

pub fn draw_scene_panel(ui: &mut Ui, info: &SceneInfo) {
    let tc = theme_colors(ui.ctx());

    // ── Zone 1: Scene Management ──

    // Save row (matches preset panel pattern)
    let mut scene_save_name: String = ui
        .ctx()
        .data_mut(|d| d.get_temp(egui::Id::new("scene_save_name")).unwrap_or_default());

    ui.horizontal(|ui| {
        let save_width = 44.0;
        let spacing = ui.spacing().item_spacing.x;
        let text_width = (ui.available_width() - save_width - spacing).max(1.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut scene_save_name)
                .hint_text("Scene name...")
                .desired_width(text_width)
                .font(egui::FontId::proportional(SMALL_SIZE)),
        );
        let can_save = !scene_save_name.trim().is_empty();
        let save_btn = ui.add_enabled(
            can_save,
            egui::Button::new(RichText::new("SAVE").size(SMALL_SIZE).strong()),
        );
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

        let available_w = ui.available_width();

        for (i, name) in info.scene_store_names.iter().enumerate() {
            let is_current = info.current_scene == Some(i);
            let card_fill = if is_current {
                tc.accent.linear_multiply(0.15)
            } else {
                tc.card_bg
            };
            let border = if is_current {
                Stroke::new(1.0, tc.accent)
            } else {
                Stroke::new(1.0, tc.card_border)
            };

            egui::Frame::new()
                .fill(card_fill)
                .stroke(border)
                .corner_radius(CornerRadius::same(WIDGET_ROUNDING))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .outer_margin(egui::Margin::symmetric(0, 1))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let is_armed =
                            pending_delete.map_or(false, |(idx, _)| idx == i);

                        // Name button
                        let display = truncate_scene_name(name, 22);
                        let label = if is_current {
                            RichText::new(&display)
                                .size(SMALL_SIZE)
                                .color(tc.accent)
                                .strong()
                        } else {
                            RichText::new(&display)
                                .size(SMALL_SIZE)
                                .color(tc.text_primary)
                        };
                        let name_w = available_w - 20.0 - 6.0 * 2.0 - 2.0 * 2.0
                            - ui.spacing().item_spacing.x;
                        let btn = egui::Button::new(label).frame(false);
                        if ui
                            .add_sized(
                                Vec2::new(name_w.max(40.0), MIN_INTERACT_HEIGHT),
                                btn,
                            )
                            .on_hover_text(name)
                            .clicked()
                        {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("load_scene"), i);
                            });
                        }

                        // Delete button
                        let del_text = if is_armed { "Sure?" } else { "×" };
                        let del_color =
                            if is_armed { tc.error } else { tc.text_secondary };
                        let del_btn = egui::Button::new(
                            RichText::new(del_text).size(SMALL_SIZE).color(del_color),
                        )
                        .frame(false);
                        if ui
                            .add_sized(
                                Vec2::new(20.0, MIN_INTERACT_HEIGHT),
                                del_btn,
                            )
                            .clicked()
                        {
                            if is_armed {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("delete_scene"),
                                        i,
                                    );
                                    d.remove_temp::<(usize, f64)>(egui::Id::new(
                                        "pending_delete_scene",
                                    ));
                                });
                            } else {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("pending_delete_scene"),
                                        (i, now),
                                    );
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
                    d.remove_temp::<(usize, f64)>(egui::Id::new(
                        "pending_delete_scene",
                    ));
                });
            }
        }
    }

    // ── Zone 2: Transport + Mode (gated on current scene) ──

    if info.current_scene.is_some() {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);
        ui.label(
            RichText::new("TRANSPORT")
                .size(HEADING_SIZE)
                .color(tc.text_secondary)
                .strong(),
        );
        ui.add_space(2.0);

        // Transport controls
        if let Some(ref tl) = info.timeline {
            if !tl.active {
                // Idle: full-width PLAY ghost-border button
                let play_btn = egui::Button::new(
                    RichText::new("PLAY")
                        .size(BODY_SIZE)
                        .color(tc.success)
                        .strong(),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, tc.success))
                .corner_radius(CornerRadius::same(WIDGET_ROUNDING));
                if ui
                    .add_sized(
                        Vec2::new(ui.available_width(), MIN_INTERACT_HEIGHT),
                        play_btn,
                    )
                    .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("scene_toggle_play"),
                            true,
                        );
                    });
                }
            } else {
                // Active: STOP | PREV | GO
                ui.horizontal(|ui| {
                    let stop_btn = egui::Button::new(
                        RichText::new("STOP")
                            .size(SMALL_SIZE)
                            .color(tc.error),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, tc.error))
                    .corner_radius(CornerRadius::same(WIDGET_ROUNDING));
                    if ui
                        .add_sized(Vec2::new(60.0, MIN_INTERACT_HEIGHT), stop_btn)
                        .clicked()
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("scene_toggle_play"),
                                true,
                            );
                        });
                    }

                    let prev_btn = egui::Button::new(
                        RichText::new("PREV").size(SMALL_SIZE).color(tc.text_primary),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, tc.card_border))
                    .corner_radius(CornerRadius::same(WIDGET_ROUNDING));
                    if ui
                        .add_sized(Vec2::new(50.0, MIN_INTERACT_HEIGHT), prev_btn)
                        .clicked()
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("scene_go_prev"),
                                true,
                            );
                        });
                    }

                    let go_w = ui.available_width();
                    let go_btn = egui::Button::new(
                        RichText::new("GO")
                            .size(BODY_SIZE)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(tc.accent)
                    .corner_radius(CornerRadius::same(WIDGET_ROUNDING));
                    if ui
                        .add_sized(Vec2::new(go_w, MIN_INTERACT_HEIGHT), go_btn)
                        .clicked()
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("scene_go_next"),
                                true,
                            );
                        });
                    }
                });
            }

            ui.add_space(2.0);

            // Loop + Advance mode on one row
            ui.horizontal(|ui| {
                let mut loop_mode = tl.loop_mode;
                if ui
                    .checkbox(&mut loop_mode, RichText::new("Loop").size(SMALL_SIZE))
                    .changed()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("scene_set_loop"),
                            loop_mode,
                        );
                    });
                }

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let advance_mode = &tl.advance_mode;
                        let mode_id: u32 = match advance_mode {
                            AdvanceMode::Manual => 0,
                            AdvanceMode::Timer => 1,
                            AdvanceMode::BeatSync { .. } => 2,
                        };
                        let mode_names = ["Manual", "Timer", "Beat Sync"];

                        let mut selected = mode_id;
                        egui::ComboBox::from_id_salt("advance_mode_combo")
                            .width(80.0)
                            .selected_text(mode_names[selected as usize])
                            .show_ui(ui, |ui| {
                                for (i, name) in mode_names.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut selected,
                                        i as u32,
                                        *name,
                                    );
                                }
                            });
                        if selected != mode_id {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new("scene_set_advance_mode"),
                                    selected,
                                );
                            });
                        }

                        ui.label(
                            RichText::new("Advance:")
                                .size(SMALL_SIZE)
                                .color(tc.text_secondary),
                        );
                    },
                );
            });

            // BeatSync: beats_per_cue control
            if let AdvanceMode::BeatSync { beats_per_cue } = &tl.advance_mode {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Beats/cue:")
                            .size(SMALL_SIZE)
                            .color(tc.text_secondary),
                    );
                    let bpc_id = egui::Id::new("beats_per_cue_val");
                    let mut bpc: u32 = ui
                        .ctx()
                        .data_mut(|d| d.get_temp(bpc_id).unwrap_or(*beats_per_cue));
                    let drag = ui.add(
                        egui::DragValue::new(&mut bpc).range(1..=64).speed(0.1),
                    );
                    if drag.changed() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(bpc_id, bpc);
                            d.insert_temp(
                                egui::Id::new("scene_set_beats_per_cue"),
                                bpc,
                            );
                        });
                    }
                });
            }

            ui.add_space(4.0);
        }

        // ── Zone 3: Cue List ──

        ui.separator();
        ui.add_space(2.0);
        ui.label(
            RichText::new("CUE LIST")
                .size(HEADING_SIZE)
                .color(tc.text_secondary)
                .strong(),
        );
        ui.add_space(2.0);

        if info.cue_list.is_empty() {
            egui::Frame::new()
                .fill(tc.card_bg)
                .stroke(Stroke::new(1.0, tc.card_border))
                .corner_radius(CornerRadius::same(WIDGET_ROUNDING))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("No cues — add presets below")
                            .size(SMALL_SIZE)
                            .color(tc.text_secondary),
                    );
                });
        } else {
            let current_cue =
                info.timeline.as_ref().map(|t| t.current_cue).unwrap_or(0);
            let active = info
                .timeline
                .as_ref()
                .map(|t| t.active)
                .unwrap_or(false);
            let transitioning_to =
                info.timeline.as_ref().and_then(|t| {
                    if let TimelineInfoState::Transitioning { to, .. } = &t.state {
                        Some(*to)
                    } else {
                        None
                    }
                });

            for (idx, cue) in info.cue_list.iter().enumerate() {
                let is_current = active && idx == current_cue;
                let is_target = transitioning_to == Some(idx);
                let card_fill = if is_current {
                    tc.accent.linear_multiply(0.15)
                } else if is_target {
                    tc.accent.linear_multiply(0.08)
                } else {
                    tc.card_bg
                };
                let border_color = if is_current {
                    Stroke::new(1.0, tc.accent)
                } else if is_target {
                    Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(
                            tc.accent.r(),
                            tc.accent.g(),
                            tc.accent.b(),
                            128,
                        ),
                    )
                } else {
                    Stroke::new(1.0, tc.card_border)
                };

                egui::Frame::new()
                    .fill(card_fill)
                    .stroke(border_color)
                    .corner_radius(CornerRadius::same(WIDGET_ROUNDING))
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .outer_margin(egui::Margin::symmetric(0, 1))
                    .show(ui, |ui| {
                        // Row 1: number | name | transition badge | duration | delete
                        ui.horizontal(|ui| {
                            // Cue number
                            ui.add_sized(
                                Vec2::new(18.0, MIN_INTERACT_HEIGHT),
                                egui::Label::new(
                                    RichText::new(format!("{}.", idx + 1))
                                        .size(SMALL_SIZE)
                                        .color(tc.text_secondary),
                                ),
                            );

                            // Click preset name to jump
                            let name_color =
                                if is_current { tc.accent } else { tc.text_primary };
                            let display =
                                truncate_scene_name(&cue.preset_name, 22);
                            let name_btn = egui::Button::new(
                                RichText::new(&display)
                                    .size(SMALL_SIZE)
                                    .color(name_color),
                            )
                            .frame(false);
                            // Calculate remaining width for name
                            // right side: transition(48) + duration(38 if non-Cut) + delete(16) + spacing
                            let right_w = 48.0
                                + if cue.transition != TransitionType::Cut {
                                    38.0
                                } else {
                                    0.0
                                }
                                + 16.0
                                + ui.spacing().item_spacing.x * 3.0;
                            let name_w = (ui.available_width() - right_w).max(30.0);
                            if ui
                                .add_sized(
                                    Vec2::new(name_w, MIN_INTERACT_HEIGHT),
                                    name_btn,
                                )
                                .on_hover_text(&cue.preset_name)
                                .clicked()
                            {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("scene_jump_to_cue"),
                                        idx,
                                    );
                                });
                            }

                            // Transition badge (ghost-border with color per type)
                            let trans_color = match cue.transition {
                                TransitionType::Cut => tc.text_secondary,
                                TransitionType::Dissolve => tc.accent,
                                TransitionType::ParamMorph => tc.success,
                            };
                            let trans_btn = egui::Button::new(
                                RichText::new(cue.transition.display_name())
                                    .size(SMALL_SIZE)
                                    .color(trans_color),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, trans_color))
                            .corner_radius(CornerRadius::same(
                                WIDGET_ROUNDING,
                            ));
                            if ui
                                .add_sized(
                                    Vec2::new(48.0, MIN_INTERACT_HEIGHT),
                                    trans_btn,
                                )
                                .on_hover_text(
                                    "Transition IN to this cue (click to cycle)",
                                )
                                .clicked()
                            {
                                let next = match cue.transition {
                                    TransitionType::Cut => {
                                        TransitionType::Dissolve
                                    }
                                    TransitionType::Dissolve => {
                                        TransitionType::ParamMorph
                                    }
                                    TransitionType::ParamMorph => {
                                        TransitionType::Cut
                                    }
                                };
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new(
                                            "scene_set_cue_transition",
                                        ),
                                        (idx, next),
                                    );
                                });
                            }

                            // Editable transition duration (only for non-Cut)
                            if cue.transition != TransitionType::Cut {
                                let dur_id =
                                    egui::Id::new("cue_dur").with(idx);
                                let mut dur: f32 = ui.ctx().data_mut(|d| {
                                    d.get_temp(dur_id)
                                        .unwrap_or(cue.transition_secs)
                                });
                                let drag = ui.add_sized(
                                    Vec2::new(38.0, MIN_INTERACT_HEIGHT),
                                    egui::DragValue::new(&mut dur)
                                        .range(0.1..=30.0)
                                        .speed(0.05)
                                        .suffix("s")
                                        .max_decimals(1),
                                );
                                if drag.changed() {
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(dur_id, dur);
                                        d.insert_temp(
                                            egui::Id::new(
                                                "scene_set_cue_transition_secs",
                                            ),
                                            (idx, dur),
                                        );
                                    });
                                }
                            }

                            // Remove cue button
                            let del_btn = egui::Button::new(
                                RichText::new("×")
                                    .size(SMALL_SIZE)
                                    .color(tc.text_secondary),
                            )
                            .frame(false);
                            if ui
                                .add_sized(
                                    Vec2::new(16.0, MIN_INTERACT_HEIGHT),
                                    del_btn,
                                )
                                .clicked()
                            {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("scene_remove_cue"),
                                        idx,
                                    );
                                });
                            }
                        });

                        // Row 2: Per-cue hold time (shown in Timer mode)
                        let is_timer =
                            info.timeline.as_ref().map_or(false, |t| {
                                matches!(t.advance_mode, AdvanceMode::Timer)
                            });
                        if is_timer {
                            ui.horizontal(|ui| {
                                ui.add_space(18.0);
                                ui.label(
                                    RichText::new("Hold:")
                                        .size(SMALL_SIZE)
                                        .color(tc.text_secondary),
                                );
                                let hold_id =
                                    egui::Id::new("cue_hold").with(idx);
                                let mut hold: f32 = ui.ctx().data_mut(|d| {
                                    d.get_temp(hold_id)
                                        .unwrap_or(cue.hold_secs.unwrap_or(4.0))
                                });
                                let drag = ui.add(
                                    egui::DragValue::new(&mut hold)
                                        .range(0.5..=120.0)
                                        .speed(0.1)
                                        .suffix("s")
                                        .max_decimals(1),
                                );
                                if drag.changed() {
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(hold_id, hold);
                                        d.insert_temp(
                                            egui::Id::new(
                                                "scene_set_cue_hold_secs",
                                            ),
                                            (idx, hold),
                                        );
                                    });
                                }
                            });
                        }
                    });
            }
        }

        // Add Cue from preset
        ui.add_space(4.0);
        if !info.preset_names.is_empty() {
            let mut add_cue_preset: usize = ui
                .ctx()
                .data_mut(|d| {
                    d.get_temp(egui::Id::new("add_cue_preset_idx"))
                        .unwrap_or(0)
                });

            ui.horizontal(|ui| {
                let btn_width = 52.0;
                let spacing = ui.spacing().item_spacing.x;
                let combo_w =
                    (ui.available_width() - btn_width - spacing).max(60.0);

                egui::ComboBox::from_id_salt("add_cue_combo")
                    .width(combo_w)
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

                let add_btn = egui::Button::new(
                    RichText::new("+ Cue")
                        .size(SMALL_SIZE)
                        .color(tc.accent),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, tc.card_border))
                .corner_radius(CornerRadius::same(WIDGET_ROUNDING))
                .min_size(Vec2::new(btn_width, MIN_INTERACT_HEIGHT));
                if ui.add(add_btn).clicked() {
                    if let Some(name) = info.preset_names.get(add_cue_preset) {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("scene_add_cue"),
                                name.clone(),
                            );
                        });
                    }
                }
            });

            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("add_cue_preset_idx"),
                    add_cue_preset,
                );
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

fn truncate_scene_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max_chars - 1).collect();
        format!("{}…", truncated)
    }
}
