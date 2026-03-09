use std::collections::HashSet;

use egui::{Color32, Pos2, RichText, Ui};

use crate::bindings::bus::BindingBus;
use crate::bindings::templates;
use crate::bindings::types::*;
use crate::ui::theme::colors::theme_colors;
use crate::ui::widgets;

// JSX-aligned source colors
const AUDIO_COLOR: Color32 = Color32::from_rgb(0x50, 0xC0, 0x70); // green
const MIDI_COLOR: Color32 = Color32::from_rgb(0xA0, 0x60, 0xD0); // purple
const OSC_COLOR: Color32 = Color32::from_rgb(0x50, 0x90, 0xE0); // blue
const WS_COLOR: Color32 = Color32::from_rgb(0xE0, 0x90, 0x40); // orange

/// Context passed to the bindings panel for building target/source pickers.
pub struct BindingPanelInfo {
    /// Effect name on the active layer (e.g. "Phosphor").
    pub effect_name: String,
    /// Param names available on the active layer (Float and Bool only).
    pub param_names: Vec<String>,
    /// Number of layers.
    pub layer_count: usize,
    /// Current preset name (for preset-scoped bindings).
    pub preset_name: String,
}

/// Draw the bindings panel (left sidebar section).
pub fn draw_bindings_panel(ui: &mut Ui, bus: &mut BindingBus, info: &BindingPanelInfo) {
    let tc = theme_colors(ui.ctx());

    // Header: source legend with uppercase labels
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        source_dot(ui, AUDIO_COLOR, "AUDIO", bus.has_source_type("audio."));
        source_dot(ui, MIDI_COLOR, "MIDI", bus.has_source_type("midi."));
        source_dot(ui, OSC_COLOR, "OSC", bus.has_source_type("osc."));
        source_dot(ui, WS_COLOR, "WS", bus.has_source_type("ws."));
    });
    ui.add_space(4.0);

    // Action bar: [+ New Binding] + [Templates]
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(RichText::new("+ New Binding").size(9.0))
                    .min_size(egui::vec2(0.0, 18.0)),
            )
            .clicked()
        {
            bus.add_binding(String::new(), String::new(), BindingScope::Preset);
        }

        egui::ComboBox::from_id_salt("bind_templates")
            .selected_text(RichText::new("Templates").size(9.0))
            .width(100.0)
            .show_ui(ui, |ui| {
                for tmpl in templates::builtin_templates() {
                    if ui
                        .button(RichText::new(tmpl.name).size(9.0))
                        .on_hover_text(tmpl.description)
                        .clicked()
                    {
                        bus.apply_template(tmpl, &info.effect_name, &info.param_names);
                    }
                }
            });
    });
    ui.add_space(4.0);

    // Collect IDs by scope
    let preset_ids: Vec<BindingId> = bus
        .bindings
        .iter()
        .filter(|b| b.scope == BindingScope::Preset)
        .map(|b| b.id.clone())
        .collect();
    let global_ids: Vec<BindingId> = bus
        .bindings
        .iter()
        .filter(|b| b.scope == BindingScope::Global)
        .map(|b| b.id.clone())
        .collect();

    let targets = build_target_options(info);

    // Preset section (always shown)
    let preset_title = format!("PRESET \u{00b7} {}", info.preset_name);
    let preset_badge = if preset_ids.is_empty() {
        None
    } else {
        Some(format!("{}", preset_ids.len()))
    };
    widgets::subsection(
        ui,
        "bind_preset_sec",
        &preset_title,
        preset_badge.as_deref(),
        tc.text_secondary,
        true,
        |ui| {
            if preset_ids.is_empty() {
                ui.label(
                    RichText::new("No preset bindings")
                        .size(9.0)
                        .color(tc.text_secondary),
                );
            } else {
                let mut to_remove = Vec::new();
                for id in &preset_ids {
                    if let Some(true) = draw_binding_row(ui, bus, id, info, &targets) {
                        to_remove.push(id.clone());
                    }
                }
                for id in to_remove {
                    bus.remove_binding(&id);
                }
            }
        },
    );

    // Global section (always shown)
    let global_badge = if global_ids.is_empty() {
        None
    } else {
        Some(format!("{}", global_ids.len()))
    };
    widgets::subsection(
        ui,
        "bind_global_sec",
        "GLOBAL",
        global_badge.as_deref(),
        tc.text_secondary,
        true,
        |ui| {
            if global_ids.is_empty() {
                ui.label(
                    RichText::new("No global bindings")
                        .size(9.0)
                        .color(tc.text_secondary),
                );
            } else {
                let mut to_remove = Vec::new();
                for id in &global_ids {
                    if let Some(true) = draw_binding_row(ui, bus, id, info, &targets) {
                        to_remove.push(id.clone());
                    }
                }
                for id in to_remove {
                    bus.remove_binding(&id);
                }
            }
        },
    );

    // Footer: source/binding/target counts
    let unique_targets: HashSet<&str> = bus
        .bindings
        .iter()
        .filter(|b| !b.target.is_empty())
        .map(|b| b.target.as_str())
        .collect();
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} sources \u{00b7} {} bindings \u{00b7} {} targets",
            bus.last_snapshot.len(),
            bus.bindings.len(),
            unique_targets.len(),
        ))
        .size(8.0)
        .color(tc.text_secondary),
    );
}

// ---------------------------------------------------------------------------
// Target options
// ---------------------------------------------------------------------------

struct TargetOption {
    id: String,
    label: String,
    group: &'static str,
}

fn build_target_options(info: &BindingPanelInfo) -> Vec<TargetOption> {
    let mut targets = Vec::new();

    // Params (active layer)
    for name in &info.param_names {
        targets.push(TargetOption {
            id: format!("param.{}.{}", info.effect_name, name),
            label: name.clone(),
            group: "Params",
        });
    }

    // Layer targets
    for i in 0..info.layer_count {
        for (suffix, label_suffix) in [("opacity", "opacity"), ("blend", "blend"), ("enabled", "enabled")] {
            targets.push(TargetOption {
                id: format!("layer.{i}.{suffix}"),
                label: format!("Layer {i} {label_suffix}"),
                group: "Layers",
            });
        }
    }

    // PostFX targets
    for (id, label) in [
        ("postfx.bloom_threshold", "Bloom threshold"),
        ("postfx.bloom_intensity", "Bloom intensity"),
        ("postfx.vignette", "Vignette"),
        ("postfx.ca_intensity", "Chromatic aberration"),
        ("postfx.grain_intensity", "Film grain"),
    ] {
        targets.push(TargetOption {
            id: id.into(),
            label: label.into(),
            group: "PostFX",
        });
    }

    // Uniform targets (direct shader uniform override)
    for (field, label) in UNIFORM_TARGETS {
        targets.push(TargetOption {
            id: format!("uniform.{field}"),
            label: label.to_string(),
            group: "Uniforms",
        });
    }

    // Scene transport
    for (id, label) in [
        ("scene.transport.go", "Next cue"),
        ("scene.transport.prev", "Previous cue"),
        ("scene.transport.stop", "Stop scene"),
    ] {
        targets.push(TargetOption {
            id: id.into(),
            label: label.into(),
            group: "Scene",
        });
    }

    // Global
    targets.push(TargetOption {
        id: "global.master_opacity".into(),
        label: "Master opacity".into(),
        group: "Global",
    });

    targets
}

/// Bindable shader uniform fields: (field_name, display_label).
const UNIFORM_TARGETS: &[(&str, &str)] = &[
    ("sub_bass", "u.sub_bass"),
    ("bass", "u.bass"),
    ("low_mid", "u.low_mid"),
    ("mid", "u.mid"),
    ("upper_mid", "u.upper_mid"),
    ("presence", "u.presence"),
    ("brilliance", "u.brilliance"),
    ("rms", "u.rms"),
    ("kick", "u.kick"),
    ("centroid", "u.centroid"),
    ("flux", "u.flux"),
    ("flatness", "u.flatness"),
    ("rolloff", "u.rolloff"),
    ("bandwidth", "u.bandwidth"),
    ("zcr", "u.zcr"),
    ("onset", "u.onset"),
    ("beat", "u.beat"),
    ("beat_phase", "u.beat_phase"),
    ("bpm", "u.bpm"),
    ("beat_strength", "u.beat_strength"),
    ("dominant_chroma", "u.dominant_chroma"),
    ("feedback_decay", "u.feedback_decay"),
    ("time", "u.time"),
];

// ---------------------------------------------------------------------------
// Source display helpers
// ---------------------------------------------------------------------------

/// Metadata for an audio source entry in the picker.
struct AudioSourceInfo {
    /// Display name shown in the picker (e.g., "Sub Bass", "kick", "MFCC 5").
    friendly: String,
    /// WGSL uniform reference (e.g., "u.sub_bass", "u.mfcc[5]").
    uniform: String,
    /// Sub-group within Audio (Bands, Features, Beat, MFCC, Chroma).
    sub_group: &'static str,
}

/// Get display metadata for an audio source key.
fn audio_source_info(key: &str) -> AudioSourceInfo {
    match key {
        // Bands
        "audio.band.0" => AudioSourceInfo {
            friendly: "Sub Bass".into(),
            uniform: "u.sub_bass".into(),
            sub_group: "Bands",
        },
        "audio.band.1" => AudioSourceInfo {
            friendly: "Bass".into(),
            uniform: "u.bass".into(),
            sub_group: "Bands",
        },
        "audio.band.2" => AudioSourceInfo {
            friendly: "Low Mid".into(),
            uniform: "u.low_mid".into(),
            sub_group: "Bands",
        },
        "audio.band.3" => AudioSourceInfo {
            friendly: "Mid".into(),
            uniform: "u.mid".into(),
            sub_group: "Bands",
        },
        "audio.band.4" => AudioSourceInfo {
            friendly: "Upper Mid".into(),
            uniform: "u.upper_mid".into(),
            sub_group: "Bands",
        },
        "audio.band.5" => AudioSourceInfo {
            friendly: "Presence".into(),
            uniform: "u.presence".into(),
            sub_group: "Bands",
        },
        "audio.band.6" => AudioSourceInfo {
            friendly: "Brilliance".into(),
            uniform: "u.brilliance".into(),
            sub_group: "Bands",
        },
        "audio.rms" => AudioSourceInfo {
            friendly: "RMS".into(),
            uniform: "u.rms".into(),
            sub_group: "Bands",
        },
        // Features
        "audio.kick" => AudioSourceInfo {
            friendly: "Kick".into(),
            uniform: "u.kick".into(),
            sub_group: "Features",
        },
        "audio.centroid" => AudioSourceInfo {
            friendly: "Centroid".into(),
            uniform: "u.centroid".into(),
            sub_group: "Features",
        },
        "audio.flux" => AudioSourceInfo {
            friendly: "Flux".into(),
            uniform: "u.flux".into(),
            sub_group: "Features",
        },
        "audio.flatness" => AudioSourceInfo {
            friendly: "Flatness".into(),
            uniform: "u.flatness".into(),
            sub_group: "Features",
        },
        "audio.rolloff" => AudioSourceInfo {
            friendly: "Rolloff".into(),
            uniform: "u.rolloff".into(),
            sub_group: "Features",
        },
        "audio.bandwidth" => AudioSourceInfo {
            friendly: "Bandwidth".into(),
            uniform: "u.bandwidth".into(),
            sub_group: "Features",
        },
        "audio.zcr" => AudioSourceInfo {
            friendly: "ZCR".into(),
            uniform: "u.zcr".into(),
            sub_group: "Features",
        },
        // Beat
        "audio.onset" => AudioSourceInfo {
            friendly: "Onset".into(),
            uniform: "u.onset".into(),
            sub_group: "Beat",
        },
        "audio.beat" => AudioSourceInfo {
            friendly: "Beat".into(),
            uniform: "u.beat".into(),
            sub_group: "Beat",
        },
        "audio.beat_phase" => AudioSourceInfo {
            friendly: "Beat Phase".into(),
            uniform: "u.beat_phase".into(),
            sub_group: "Beat",
        },
        "audio.bpm" => AudioSourceInfo {
            friendly: "BPM".into(),
            uniform: "u.bpm".into(),
            sub_group: "Beat",
        },
        "audio.beat_strength" => AudioSourceInfo {
            friendly: "Beat Strength".into(),
            uniform: "u.beat_strength".into(),
            sub_group: "Beat",
        },
        // Chroma
        "audio.dominant_chroma" => AudioSourceInfo {
            friendly: "Dominant Chroma".into(),
            uniform: "u.dominant_chroma".into(),
            sub_group: "Chroma",
        },
        _ => {
            // Dynamic: mfcc.N, chroma.N
            if let Some(n) = key.strip_prefix("audio.mfcc.") {
                return AudioSourceInfo {
                    friendly: format!("MFCC {n}"),
                    uniform: format!("u.mfcc[{n}]"),
                    sub_group: "MFCC",
                };
            }
            if let Some(n) = key.strip_prefix("audio.chroma.") {
                let note = match n {
                    "0" => "C",
                    "1" => "C#",
                    "2" => "D",
                    "3" => "D#",
                    "4" => "E",
                    "5" => "F",
                    "6" => "F#",
                    "7" => "G",
                    "8" => "G#",
                    "9" => "A",
                    "10" => "A#",
                    "11" => "B",
                    _ => n,
                };
                return AudioSourceInfo {
                    friendly: format!("Chroma {note}"),
                    uniform: format!("u.chroma[{n}]"),
                    sub_group: "Chroma",
                };
            }
            // Fallback
            let short = key.strip_prefix("audio.").unwrap_or(key);
            AudioSourceInfo {
                friendly: short.to_string(),
                uniform: format!("u.{short}"),
                sub_group: "Other",
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Binding row (collapsed)
// ---------------------------------------------------------------------------

/// Draw a single binding row. Returns Some(true) if should be deleted.
fn draw_binding_row(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    info: &BindingPanelInfo,
    targets: &[TargetOption],
) -> Option<bool> {
    let tc = theme_colors(ui.ctx());

    let binding = bus.get_binding(id)?.clone();
    let runtime = bus.runtime(id);
    let last_output = runtime.and_then(|r| r.last_output);

    let expanded_id = egui::Id::new(format!("bind_expand_{}", id));
    let mut expanded = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(expanded_id).unwrap_or(false));

    let mut should_remove = false;
    let src_color = source_color(&binding.source);
    let opacity = if binding.enabled { 1.0 } else { 0.5 };

    // Two-line collapsed row inside a group with accent bar
    let group_resp = ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.multiply_opacity(opacity);

        // Line 1: dot, source badge, name, scope badge
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Enable dot (clickable)
            let dot_color = if binding.enabled {
                src_color
            } else {
                Color32::from_rgb(0x33, 0x33, 0x33)
            };
            let (dot_rect, dot_resp) =
                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::click());
            ui.painter()
                .circle_filled(dot_rect.center(), 3.5, dot_color);
            if dot_resp.clicked() {
                if let Some(b) = bus.get_binding_mut(id) {
                    b.enabled = !b.enabled;
                }
            }
            dot_resp.on_hover_text(if binding.enabled {
                "Disable"
            } else {
                "Enable"
            });

            // Source badge (colored pill)
            draw_source_badge(ui, &binding.source);

            // Binding name (clickable to expand/collapse)
            let display_name = make_display_name(&binding.source, &binding.target);
            let name_label = if binding.name.is_empty() {
                &display_name
            } else {
                &binding.name
            };
            if ui
                .add(
                    egui::Label::new(
                        RichText::new(name_label).size(9.0).color(tc.text_primary),
                    )
                    .sense(egui::Sense::click()),
                )
                .clicked()
            {
                expanded = !expanded;
            }

            // Right-aligned scope badge
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let scope_text = match binding.scope {
                    BindingScope::Preset => "PRE",
                    BindingScope::Global => "GLB",
                };
                ui.label(
                    RichText::new(scope_text)
                        .size(7.0)
                        .color(tc.text_secondary),
                );
            });
        });

        // Line 2: output value + mini bar + transform chain (only when has data)
        let has_transforms = !binding.transforms.is_empty();
        if last_output.is_some() || has_transforms {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(12.0); // indent past dot

                if let Some(val) = last_output {
                    ui.label(
                        RichText::new(format!("{:.2}", val))
                            .size(8.0)
                            .color(src_color.linear_multiply(0.8)),
                    );

                    // Mini bar (48x4)
                    let (bar_rect, _) =
                        ui.allocate_exact_size(egui::vec2(48.0, 4.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        bar_rect,
                        1.0,
                        Color32::from_rgb(0x22, 0x22, 0x22),
                    );
                    let filled = egui::Rect::from_min_size(
                        bar_rect.min,
                        egui::vec2(
                            bar_rect.width() * val.clamp(0.0, 1.0),
                            bar_rect.height(),
                        ),
                    );
                    ui.painter()
                        .rect_filled(filled, 1.0, src_color.linear_multiply(0.7));
                }

                if has_transforms {
                    let chain: Vec<String> = binding
                        .transforms
                        .iter()
                        .map(|t| transform_short_label(t))
                        .collect();
                    ui.label(
                        RichText::new(chain.join(" \u{2192} "))
                            .size(7.0)
                            .color(tc.text_secondary),
                    );
                }
            });
        }
    });

    // Paint accent bar on left edge
    let rect = group_resp.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.left_top(), egui::vec2(3.0, rect.height())),
        1.0,
        if binding.enabled {
            src_color.linear_multiply(0.6)
        } else {
            Color32::from_rgb(0x33, 0x33, 0x33)
        },
    );

    // Expanded details
    if expanded {
        ui.indent(format!("bind_detail_{id}"), |ui| {
            should_remove = draw_binding_details(ui, bus, id, info, targets);
        });
    }

    ui.ctx().data_mut(|d| d.insert_temp(expanded_id, expanded));
    ui.add_space(2.0);

    Some(should_remove)
}

/// Draw a colored source badge pill (AUD/MID/OSC/WS).
fn draw_source_badge(ui: &mut Ui, source: &str) {
    let (abbrev, color) = source_badge_info(source);
    ui.add(
        egui::Button::new(
            RichText::new(abbrev)
                .size(7.0)
                .color(Color32::WHITE)
                .strong(),
        )
        .fill(color.linear_multiply(0.7))
        .corner_radius(3.0)
        .min_size(egui::vec2(0.0, 12.0))
        .sense(egui::Sense::hover()),
    );
}

fn source_badge_info(source: &str) -> (&'static str, Color32) {
    if source.starts_with("audio.") {
        ("AUD", AUDIO_COLOR)
    } else if source.starts_with("midi.") {
        ("MID", MIDI_COLOR)
    } else if source.starts_with("osc.") {
        ("OSC", OSC_COLOR)
    } else if source.starts_with("ws.") {
        ("WS", WS_COLOR)
    } else {
        ("---", Color32::GRAY)
    }
}

// ---------------------------------------------------------------------------
// Expanded binding editor
// ---------------------------------------------------------------------------

fn draw_binding_details(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    _info: &BindingPanelInfo,
    targets: &[TargetOption],
) -> bool {
    let tc = theme_colors(ui.ctx());

    let Some(binding) = bus.get_binding(id) else {
        return false;
    };
    let mut source = binding.source.clone();
    let mut target = binding.target.clone();
    let mut name = binding.name.clone();
    let mut enabled = binding.enabled;
    let mut scope = binding.scope.clone();
    let transforms = binding.transforms.clone();

    // Preview: Raw / Norm / Out
    if let Some(runtime) = bus.runtime(id) {
        let src_color = source_color(&source);
        ui.add_space(2.0);

        if let Some(input) = runtime.last_input {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new("Raw").size(8.0).color(tc.text_secondary));
                if let Some(ref raw) = runtime.last_raw {
                    ui.label(
                        RichText::new(&raw.display)
                            .size(8.0)
                            .color(tc.text_secondary),
                    );
                }
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new("Norm").size(8.0).color(tc.text_secondary));
                draw_inline_bar(ui, input, 60.0, 6.0, tc.text_secondary);
                ui.label(
                    RichText::new(format!("{:.2}", input))
                        .size(8.0)
                        .color(tc.text_secondary),
                );
            });
        }

        if let Some(output) = runtime.last_output {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(
                    RichText::new("Out")
                        .size(8.0)
                        .strong()
                        .color(src_color.linear_multiply(0.8)),
                );
                draw_inline_bar(ui, output, 60.0, 6.0, src_color.linear_multiply(0.7));
                ui.label(
                    RichText::new(format!("{:.2}", output))
                        .size(8.0)
                        .strong()
                        .color(src_color.linear_multiply(0.8)),
                );
            });
        }
        ui.add_space(2.0);
    }

    // --- Source picker ---
    draw_source_picker(ui, bus, id, &mut source, &tc);

    // --- Target picker ---
    ui.horizontal(|ui| {
        ui.label(RichText::new("Target").size(8.0).color(tc.text_secondary));

        let current_label = target_display_label(&target, targets);
        egui::ComboBox::from_id_salt(format!("target_pick_{id}"))
            .selected_text(RichText::new(&current_label).size(9.0))
            .width(200.0)
            .show_ui(ui, |ui| {
                let mut current_group: &str = "";
                for opt in targets {
                    if opt.group != current_group {
                        current_group = opt.group;
                        ui.label(
                            RichText::new(current_group)
                                .size(8.0)
                                .strong()
                                .color(tc.text_secondary),
                        );
                    }
                    let selected = target == opt.id;
                    if ui
                        .selectable_label(selected, RichText::new(&opt.label).size(9.0))
                        .clicked()
                    {
                        target = opt.id.clone();
                    }
                }
            });
    });

    // --- Transforms ---
    ui.add_space(2.0);
    ui.label(
        RichText::new("TRANSFORMS")
            .size(7.0)
            .color(tc.text_secondary),
    );
    draw_transform_editor(ui, bus, id, &transforms);

    // Scope toggle + Enabled
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut enabled, RichText::new("Enabled").size(8.0));
        ui.add_space(8.0);
        let (scope_label, scope_tip) = match scope {
            BindingScope::Preset => (
                "Preset",
                "Saved with this preset. Click to make global.",
            ),
            BindingScope::Global => (
                "Global",
                "Always active. Click to make preset-scoped.",
            ),
        };
        if ui
            .add(egui::Button::new(RichText::new(scope_label).size(8.0)))
            .on_hover_text(scope_tip)
            .clicked()
        {
            scope = match scope {
                BindingScope::Preset => BindingScope::Global,
                BindingScope::Global => BindingScope::Preset,
            };
        }
    });

    // Name field
    let auto_name = make_display_name(&source, &target);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Name").size(8.0).color(tc.text_secondary));
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .desired_width(140.0)
                .font(egui::TextStyle::Small)
                .hint_text(&auto_name),
        );
        if !name.is_empty()
            && ui
                .add(
                    egui::Button::new(RichText::new("\u{00d7}").size(8.0))
                        .min_size(egui::vec2(12.0, 12.0)),
                )
                .on_hover_text("Use auto-generated name")
                .clicked()
        {
            name.clear();
        }
    });

    // Delete button (expanded only)
    let mut should_remove = false;
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(
                    RichText::new("Delete Binding")
                        .size(8.0)
                        .color(Color32::from_rgb(0xE0, 0x60, 0x60)),
                )
                .min_size(egui::vec2(0.0, 16.0)),
            )
            .clicked()
        {
            should_remove = true;
        }
    });

    // Apply changes back
    if let Some(b) = bus.get_binding_mut(id) {
        b.source = source;
        b.target = target;
        b.name = name;
        b.enabled = enabled;
        b.scope = scope;
    }

    ui.add_space(4.0);
    should_remove
}

// ---------------------------------------------------------------------------
// Source picker (custom-painted rows with live bars + uniform refs)
// ---------------------------------------------------------------------------

/// Canonical audio source ordering for the picker (by sub-group).
const AUDIO_SOURCE_ORDER: &[&str] = &[
    // Bands
    "audio.band.0",
    "audio.band.1",
    "audio.band.2",
    "audio.band.3",
    "audio.band.4",
    "audio.band.5",
    "audio.band.6",
    "audio.rms",
    // Features
    "audio.kick",
    "audio.centroid",
    "audio.flux",
    "audio.flatness",
    "audio.rolloff",
    "audio.bandwidth",
    "audio.zcr",
    // Beat
    "audio.onset",
    "audio.beat",
    "audio.beat_phase",
    "audio.bpm",
    "audio.beat_strength",
    // Chroma
    "audio.dominant_chroma",
];

fn draw_source_picker(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    source: &mut String,
    tc: &crate::ui::theme::colors::ThemeColors,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Source").size(8.0).color(tc.text_secondary));

        let current_display = if source.is_empty() {
            "(select source)".to_string()
        } else {
            let info_str = audio_source_info(source).friendly;
            if source.starts_with("audio.") {
                info_str
            } else {
                friendly_source(source)
            }
        };

        egui::ComboBox::from_id_salt(format!("source_pick_{id}"))
            .selected_text(RichText::new(&current_display).size(9.0))
            .width(200.0)
            .show_ui(ui, |ui| {
                ui.set_min_width(260.0);
                ui.set_max_height(350.0);
                ui.spacing_mut().item_spacing.y = 0.0;

                // --- Audio sources (sorted by sub-group) ---
                let has_audio = bus.last_snapshot.keys().any(|k| k.starts_with("audio."));
                // Helper: draw a group header with top padding
                let group_header = |ui: &mut Ui, label: &str, color: Color32| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(label)
                            .size(7.0)
                            .strong()
                            .color(color.linear_multiply(0.7)),
                    );
                    ui.add_space(1.0);
                };

                if has_audio {
                    // Canonical sources (always present when audio is active)
                    let mut current_sub_group = "";
                    for &key in AUDIO_SOURCE_ORDER {
                        let info = audio_source_info(key);
                        if info.sub_group != current_sub_group {
                            current_sub_group = info.sub_group;
                            group_header(
                                ui,
                                &format!("Audio \u{2014} {current_sub_group}"),
                                AUDIO_COLOR,
                            );
                        }
                        let val = bus
                            .last_snapshot
                            .get(key)
                            .map(|(v, _)| *v)
                            .unwrap_or(0.0);
                        draw_source_row(
                            ui,
                            key,
                            &info.friendly,
                            &info.uniform,
                            val,
                            AUDIO_COLOR,
                            source.as_str() == key,
                            source,
                        );
                    }

                    // MFCC (dynamic)
                    let mut mfcc_keys: Vec<String> = bus
                        .last_snapshot
                        .keys()
                        .filter(|k| k.starts_with("audio.mfcc."))
                        .cloned()
                        .collect();
                    if !mfcc_keys.is_empty() {
                        mfcc_keys.sort_by_key(|k| {
                            k.strip_prefix("audio.mfcc.")
                                .and_then(|n| n.parse::<u32>().ok())
                                .unwrap_or(99)
                        });
                        group_header(ui, "Audio \u{2014} MFCC", AUDIO_COLOR);
                        for key in &mfcc_keys {
                            let info = audio_source_info(key);
                            let val = bus
                                .last_snapshot
                                .get(key.as_str())
                                .map(|(v, _)| *v)
                                .unwrap_or(0.0);
                            draw_source_row(
                                ui,
                                key,
                                &info.friendly,
                                &info.uniform,
                                val,
                                AUDIO_COLOR,
                                source.as_str() == key.as_str(),
                                source,
                            );
                        }
                    }

                    // Chroma (dynamic)
                    let mut chroma_keys: Vec<String> = bus
                        .last_snapshot
                        .keys()
                        .filter(|k| k.starts_with("audio.chroma."))
                        .cloned()
                        .collect();
                    if !chroma_keys.is_empty() {
                        chroma_keys.sort_by_key(|k| {
                            k.strip_prefix("audio.chroma.")
                                .and_then(|n| n.parse::<u32>().ok())
                                .unwrap_or(99)
                        });
                        group_header(ui, "Audio \u{2014} Chroma", AUDIO_COLOR);
                        for key in &chroma_keys {
                            let info = audio_source_info(key);
                            let val = bus
                                .last_snapshot
                                .get(key.as_str())
                                .map(|(v, _)| *v)
                                .unwrap_or(0.0);
                            draw_source_row(
                                ui,
                                key,
                                &info.friendly,
                                &info.uniform,
                                val,
                                AUDIO_COLOR,
                                source.as_str() == key.as_str(),
                                source,
                            );
                        }
                    }
                }

                // --- MIDI sources ---
                let mut midi_keys: Vec<&String> = bus
                    .last_snapshot
                    .keys()
                    .filter(|k| k.starts_with("midi."))
                    .collect();
                if !midi_keys.is_empty() {
                    midi_keys.sort();
                    group_header(ui, "MIDI", MIDI_COLOR);
                    for key in &midi_keys {
                        let val = bus
                            .last_snapshot
                            .get(key.as_str())
                            .map(|(v, _)| *v)
                            .unwrap_or(0.0);
                        let display = friendly_source(key);
                        draw_source_row(
                            ui,
                            key,
                            &display,
                            "",
                            val,
                            MIDI_COLOR,
                            source.as_str() == key.as_str(),
                            source,
                        );
                    }
                }

                // --- OSC sources ---
                let mut osc_keys: Vec<&String> = bus
                    .last_snapshot
                    .keys()
                    .filter(|k| k.starts_with("osc."))
                    .collect();
                if !osc_keys.is_empty() {
                    osc_keys.sort();
                    group_header(ui, "OSC", OSC_COLOR);
                    for key in &osc_keys {
                        let val = bus
                            .last_snapshot
                            .get(key.as_str())
                            .map(|(v, _)| *v)
                            .unwrap_or(0.0);
                        let display = friendly_source(key);
                        draw_source_row(
                            ui,
                            key,
                            &display,
                            "",
                            val,
                            OSC_COLOR,
                            source.as_str() == key.as_str(),
                            source,
                        );
                    }
                }

                // --- WS sources ---
                let mut ws_keys: Vec<&String> = bus
                    .last_snapshot
                    .keys()
                    .filter(|k| k.starts_with("ws."))
                    .collect();
                if !ws_keys.is_empty() {
                    ws_keys.sort();
                    group_header(ui, "WS", WS_COLOR);
                    for key in &ws_keys {
                        let val = bus
                            .last_snapshot
                            .get(key.as_str())
                            .map(|(v, _)| *v)
                            .unwrap_or(0.0);
                        let display = friendly_source(key);
                        draw_source_row(
                            ui,
                            key,
                            &display,
                            "",
                            val,
                            WS_COLOR,
                            source.as_str() == key.as_str(),
                            source,
                        );
                    }
                }
            });
    });

    // Manual source entry + Learn button
    ui.horizontal(|ui| {
        ui.add_space(36.0);
        ui.add(
            egui::TextEdit::singleline(source)
                .desired_width(120.0)
                .font(egui::TextStyle::Small)
                .hint_text("manual source key"),
        );

        let is_learning = bus
            .learn_target
            .as_ref()
            .is_some_and(|l| l.binding_id == id && l.field == LearnField::Source);
        if is_learning {
            let t = ui.input(|i| i.time) as f32;
            let alpha = ((t * 4.0).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
            let color =
                Color32::from_rgba_unmultiplied(0xE0, 0xA0, 0x40, (alpha * 255.0) as u8);
            if ui
                .add(egui::Button::new(
                    RichText::new("..").color(color).size(8.0),
                ))
                .on_hover_text("Cancel learn")
                .clicked()
            {
                bus.learn_target = None;
            }
            ui.ctx().request_repaint();
        } else if ui
            .add(egui::Button::new(RichText::new("Learn").size(8.0)))
            .on_hover_text("Learn source from next MIDI/OSC message")
            .clicked()
        {
            bus.learn_target = Some(LearnState {
                binding_id: id.to_string(),
                field: LearnField::Source,
            });
        }
    });
}

/// Draw a single custom-painted source row inside the picker popup.
/// Layout: [name ·····  ▓▓▓░░ 0.42  u.field]
fn draw_source_row(
    ui: &mut Ui,
    key: &str,
    friendly_name: &str,
    uniform_ref: &str,
    val: f32,
    color: Color32,
    selected: bool,
    source_out: &mut String,
) {
    let row_height = 18.0;
    let avail_width = ui.available_width().max(260.0);
    let desired = egui::vec2(avail_width, row_height);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

    if resp.clicked() {
        *source_out = key.to_string();
    }

    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        painter.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
    }

    let text_color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    let dim_color = Color32::from_white_alpha(60);

    // Layout: use proportional positions relative to row width
    let left = rect.left() + 6.0;
    let cy = rect.center().y;

    // Columns anchored from the right so nothing clips:
    //   [right-4]  uniform_ref (right-aligned)
    //   [right-84] value text  (right-aligned, 30px zone)
    //   [right-90] bar end
    //   [right-130] bar start (40px bar)
    // Everything left of bar_start is the name column.
    let uniform_right = rect.right() - 4.0;
    let val_right = rect.right() - 70.0;
    let bar_right = val_right - 6.0;
    let bar_width = 36.0;
    let bar_left = bar_right - bar_width;

    // Name (clip via painter clip rect isn't needed — we just let it truncate naturally)
    painter.text(
        Pos2::new(left, cy),
        egui::Align2::LEFT_CENTER,
        friendly_name,
        egui::FontId::proportional(9.0),
        text_color,
    );

    // Mini bar
    let bar_rect = egui::Rect::from_min_size(
        Pos2::new(bar_left, cy - 2.0),
        egui::vec2(bar_width, 4.0),
    );
    painter.rect_filled(bar_rect, 1.0, Color32::from_rgb(0x2a, 0x2a, 0x2a));
    let fill_w = bar_width * val.clamp(0.0, 1.0);
    if fill_w > 0.5 {
        let fill_rect =
            egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 4.0));
        painter.rect_filled(fill_rect, 1.0, color.linear_multiply(0.7));
    }

    // Value (right-aligned in its zone)
    painter.text(
        Pos2::new(val_right, cy),
        egui::Align2::LEFT_CENTER,
        &format!("{val:.2}"),
        egui::FontId::proportional(8.0),
        dim_color,
    );

    // Uniform ref (far right, very dim)
    if !uniform_ref.is_empty() {
        painter.text(
            Pos2::new(uniform_right, cy),
            egui::Align2::RIGHT_CENTER,
            uniform_ref,
            egui::FontId::proportional(7.0),
            Color32::from_white_alpha(35),
        );
    }

    resp.on_hover_text(key);
}

// ---------------------------------------------------------------------------
// Inline bar helper
// ---------------------------------------------------------------------------

fn draw_inline_bar(ui: &mut Ui, value: f32, width: f32, height: f32, fill_color: Color32) {
    let (bar_rect, _) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(
        bar_rect,
        1.0,
        Color32::from_rgb(0x22, 0x22, 0x22),
    );
    let filled = egui::Rect::from_min_size(
        bar_rect.min,
        egui::vec2(bar_rect.width() * value.clamp(0.0, 1.0), bar_rect.height()),
    );
    ui.painter().rect_filled(filled, 1.0, fill_color);
}

// ---------------------------------------------------------------------------
// Transform editor with inline params
// ---------------------------------------------------------------------------

fn draw_transform_editor(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    transforms: &[TransformDef],
) {
    let tc = theme_colors(ui.ctx());

    let mut to_remove: Option<usize> = None;
    let mut updated_transforms: Vec<TransformDef> = transforms.to_vec();

    for (i, t) in transforms.iter().enumerate() {
        let xform_edit_id = egui::Id::new(format!("xform_edit_{}_{}", id, i));
        let mut editing = ui
            .ctx()
            .data_mut(|d| d.get_temp::<bool>(xform_edit_id).unwrap_or(false));

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            let label = transform_short_label(t);
            if ui
                .add(
                    egui::Label::new(
                        RichText::new(&label)
                            .size(8.0)
                            .color(if editing {
                                tc.text_primary
                            } else {
                                tc.text_secondary
                            }),
                    )
                    .sense(egui::Sense::click()),
                )
                .on_hover_text("Click to edit parameters")
                .clicked()
            {
                editing = !editing;
            }

            if ui
                .add(
                    egui::Button::new(RichText::new("\u{00d7}").size(8.0))
                        .min_size(egui::vec2(12.0, 12.0)),
                )
                .on_hover_text("Remove transform")
                .clicked()
            {
                to_remove = Some(i);
            }
        });

        if editing {
            ui.indent(format!("xform_params_{}_{}", id, i), |ui| {
                let mut xform = updated_transforms[i].clone();
                draw_transform_params(ui, &mut xform);
                updated_transforms[i] = xform;
            });
        }

        ui.ctx().data_mut(|d| d.insert_temp(xform_edit_id, editing));
    }

    if updated_transforms != *transforms {
        if let Some(b) = bus.get_binding_mut(id) {
            b.transforms = updated_transforms;
        }
    }

    if let Some(idx) = to_remove {
        if let Some(b) = bus.get_binding_mut(id) {
            b.transforms.remove(idx);
        }
    }

    egui::ComboBox::from_id_salt(format!("add_xform_{id}"))
        .selected_text(RichText::new("+ transform").size(8.0))
        .width(100.0)
        .show_ui(ui, |ui| {
            let items = [
                ("Smooth", TransformDef::Smooth { factor: 0.8 }),
                (
                    "Remap",
                    TransformDef::Remap {
                        in_lo: 0.0,
                        in_hi: 1.0,
                        out_lo: 0.0,
                        out_hi: 1.0,
                    },
                ),
                ("Gate", TransformDef::Gate { threshold: 0.5 }),
                ("Scale", TransformDef::Scale { factor: 1.0 }),
                ("Offset", TransformDef::Offset { value: 0.0 }),
                ("Quantize", TransformDef::Quantize { steps: 4 }),
                ("Invert", TransformDef::Invert),
                (
                    "Clamp",
                    TransformDef::Clamp {
                        lo: 0.0,
                        hi: 1.0,
                    },
                ),
                (
                    "Deadzone",
                    TransformDef::Deadzone {
                        lo: 0.45,
                        hi: 0.55,
                    },
                ),
                (
                    "Curve",
                    TransformDef::Curve {
                        curve_type: "ease_in_out".into(),
                    },
                ),
            ];
            for (name, default) in items {
                if ui.button(RichText::new(name).size(9.0)).clicked() {
                    if let Some(b) = bus.get_binding_mut(id) {
                        b.transforms.push(default);
                    }
                }
            }
        });
}

fn draw_transform_params(ui: &mut Ui, t: &mut TransformDef) {
    match t {
        TransformDef::Smooth { factor } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("factor").size(8.0));
                ui.add(
                    egui::DragValue::new(factor)
                        .range(0.0..=0.999)
                        .speed(0.01)
                        .max_decimals(3),
                );
            });
        }
        TransformDef::Remap {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("in").size(8.0));
                ui.add(
                    egui::DragValue::new(in_lo)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
                ui.label(RichText::new("\u{2013}").size(8.0));
                ui.add(
                    egui::DragValue::new(in_hi)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("out").size(8.0));
                ui.add(
                    egui::DragValue::new(out_lo)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
                ui.label(RichText::new("\u{2013}").size(8.0));
                ui.add(
                    egui::DragValue::new(out_hi)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
        }
        TransformDef::Quantize { steps } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("steps").size(8.0));
                let mut s = *steps as i32;
                ui.add(egui::DragValue::new(&mut s).range(1..=64));
                *steps = s.max(1) as u32;
            });
        }
        TransformDef::Gate { threshold } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("threshold").size(8.0));
                ui.add(
                    egui::DragValue::new(threshold)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
        }
        TransformDef::Scale { factor } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("factor").size(8.0));
                ui.add(
                    egui::DragValue::new(factor)
                        .range(-10.0..=10.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
        }
        TransformDef::Offset { value } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("value").size(8.0));
                ui.add(
                    egui::DragValue::new(value)
                        .range(-1.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
        }
        TransformDef::Clamp { lo, hi } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("lo").size(8.0));
                ui.add(
                    egui::DragValue::new(lo)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
                ui.label(RichText::new("hi").size(8.0));
                ui.add(
                    egui::DragValue::new(hi)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
        }
        TransformDef::Deadzone { lo, hi } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("lo").size(8.0));
                ui.add(
                    egui::DragValue::new(lo)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
                ui.label(RichText::new("hi").size(8.0));
                ui.add(
                    egui::DragValue::new(hi)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(2),
                );
            });
        }
        TransformDef::Curve { curve_type } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("curve").size(8.0));
                egui::ComboBox::from_id_salt("curve_type_pick")
                    .selected_text(RichText::new(curve_type.as_str()).size(8.0))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for name in [
                            "linear",
                            "ease_in",
                            "ease_out",
                            "ease_in_out",
                            "log",
                            "exp",
                        ] {
                            if ui
                                .selectable_label(
                                    curve_type.as_str() == name,
                                    RichText::new(name).size(8.0),
                                )
                                .clicked()
                            {
                                *curve_type = name.to_string();
                            }
                        }
                    });
            });
        }
        TransformDef::Invert => {}
    }
}

// ---------------------------------------------------------------------------
// Display label helpers
// ---------------------------------------------------------------------------

fn transform_short_label(t: &TransformDef) -> String {
    match t {
        TransformDef::Smooth { factor } => format!("smooth({factor:.1})"),
        TransformDef::Remap {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        } => format!("remap({in_lo:.1}\u{2013}{in_hi:.1}\u{2192}{out_lo:.1}\u{2013}{out_hi:.1})"),
        TransformDef::Quantize { steps } => format!("quantize({steps})"),
        TransformDef::Gate { threshold } => format!("gate({threshold:.1})"),
        TransformDef::Scale { factor } => format!("scale({factor:.1})"),
        TransformDef::Offset { value } => format!("offset({value:.1})"),
        TransformDef::Clamp { lo, hi } => format!("clamp({lo:.1}\u{2013}{hi:.1})"),
        TransformDef::Deadzone { lo, hi } => format!("dz({lo:.1}\u{2013}{hi:.1})"),
        TransformDef::Curve { curve_type } => format!("curve({curve_type})"),
        TransformDef::Invert => "invert".into(),
    }
}

fn make_display_name(source: &str, target: &str) -> String {
    let src = if source.starts_with("audio.") {
        audio_source_info(source).friendly
    } else {
        friendly_source(source)
    };
    let tgt = friendly_target(target);
    if src.is_empty() && tgt.is_empty() {
        "(new binding)".into()
    } else if src.is_empty() {
        format!("? \u{2192} {tgt}")
    } else if tgt.is_empty() {
        format!("{src} \u{2192} ?")
    } else {
        format!("{src} \u{2192} {tgt}")
    }
}

fn friendly_source(source: &str) -> String {
    if source.is_empty() {
        return String::new();
    }
    if source.starts_with("midi.") {
        let parts: Vec<&str> = source.split('.').collect();
        if parts.len() >= 5 {
            let msg_type = parts[2];
            let cc = parts[4];
            return match msg_type {
                "cc" => format!("CC {cc}"),
                "note" => format!("Note {cc}"),
                _ => parts.last().unwrap_or(&"?").to_string(),
            };
        }
        return source
            .strip_prefix("midi.")
            .unwrap_or(source)
            .to_string();
    }
    if source.starts_with("audio.") {
        return source
            .strip_prefix("audio.")
            .unwrap_or(source)
            .to_string();
    }
    if source.starts_with("osc.") {
        let addr = source.strip_prefix("osc.").unwrap_or(source);
        return addr.rsplit('/').next().unwrap_or(addr).to_string();
    }
    if source.starts_with("ws.") {
        let rest = source.strip_prefix("ws.").unwrap_or(source);
        return rest.rsplit('.').next().unwrap_or(rest).to_string();
    }
    source.to_string()
}

fn friendly_target(target: &str) -> String {
    if target.is_empty() {
        return String::new();
    }
    let parts: Vec<&str> = target.split('.').collect();
    match parts.first().copied() {
        Some("param") => parts.get(2).unwrap_or(&"?").to_string(),
        Some("layer") => {
            let idx = parts.get(1).unwrap_or(&"?");
            let field = parts.get(2).unwrap_or(&"?");
            format!("L{idx} {field}")
        }
        Some("global") => {
            let field = parts.get(1).unwrap_or(&"?");
            field.replace('_', " ")
        }
        Some("postfx") => {
            let field = parts.get(1).unwrap_or(&"?");
            field.replace('_', " ")
        }
        Some("uniform") => {
            let field = parts.get(1).unwrap_or(&"?");
            format!("u.{field}")
        }
        Some("scene") => {
            let action = parts.get(2).unwrap_or(&"?");
            format!("scene {action}")
        }
        _ => target.to_string(),
    }
}

fn target_display_label(target: &str, targets: &[TargetOption]) -> String {
    if target.is_empty() {
        return "(select target)".into();
    }
    targets
        .iter()
        .find(|t| t.id == target)
        .map(|t| t.label.clone())
        .unwrap_or_else(|| friendly_target(target))
}

fn source_color(source: &str) -> Color32 {
    if source.starts_with("audio.") {
        AUDIO_COLOR
    } else if source.starts_with("midi.") {
        MIDI_COLOR
    } else if source.starts_with("osc.") {
        OSC_COLOR
    } else if source.starts_with("ws.") {
        WS_COLOR
    } else {
        Color32::GRAY
    }
}

fn source_dot(ui: &mut Ui, color: Color32, label: &str, active: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let c = if active {
            color
        } else {
            Color32::from_rgb(0x33, 0x33, 0x33)
        };
        let (r, _) = ui.allocate_exact_size(egui::vec2(4.0, 4.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 2.0, c);
        let dim = Color32::from_white_alpha(if active { 90 } else { 38 });
        ui.label(RichText::new(label).size(7.0).color(dim));
    });
}
