use egui::{Color32, RichText, Ui};

use crate::bindings::bus::BindingBus;
use crate::bindings::types::*;
use crate::ui::theme::colors::theme_colors;

const AUDIO_COLOR: Color32 = Color32::from_rgb(0xE0, 0x80, 0x40); // orange
const MIDI_COLOR: Color32 = Color32::from_rgb(0x60, 0xA0, 0xE0); // blue
const OSC_COLOR: Color32 = Color32::from_rgb(0x50, 0xC0, 0x70); // green
const WS_COLOR: Color32 = Color32::from_rgb(0x50, 0x90, 0xE0); // light blue

/// Context passed to the bindings panel for building target/source pickers.
pub struct BindingPanelInfo {
    /// Effect name on the active layer (e.g. "Phosphor").
    pub effect_name: String,
    /// Param names available on the active layer (Float and Bool only).
    pub param_names: Vec<String>,
    /// Number of layers.
    pub layer_count: usize,
}

/// Draw the bindings panel (left sidebar section).
pub fn draw_bindings_panel(ui: &mut Ui, bus: &mut BindingBus, info: &BindingPanelInfo) {
    let tc = theme_colors(ui.ctx());

    // Source legend row
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        source_dot(ui, AUDIO_COLOR, "Audio", bus.has_source_type("audio."));
        source_dot(ui, MIDI_COLOR, "MIDI", bus.has_source_type("midi."));
        source_dot(ui, OSC_COLOR, "OSC", bus.has_source_type("osc."));
        source_dot(ui, WS_COLOR, "WS", bus.has_source_type("ws."));
    });
    ui.add_space(4.0);

    // Add binding button
    if ui
        .add(
            egui::Button::new(RichText::new("+ New Binding").size(9.0))
                .min_size(egui::vec2(0.0, 18.0)),
        )
        .clicked()
    {
        bus.add_binding(String::new(), String::new(), BindingScope::Preset);
    }
    ui.add_space(4.0);

    // Separate into preset and global bindings
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

    // Build target options
    let targets = build_target_options(info);

    // Preset bindings section
    if !preset_ids.is_empty() {
        ui.label(
            RichText::new("Preset")
                .size(8.0)
                .color(tc.text_secondary),
        );
        let mut to_remove = Vec::new();
        for id in &preset_ids {
            if let Some(remove) = draw_binding_row(ui, bus, id, info, &targets) {
                if remove {
                    to_remove.push(id.clone());
                }
            }
        }
        for id in to_remove {
            bus.remove_binding(&id);
        }
        ui.add_space(4.0);
    }

    // Global bindings section
    if !global_ids.is_empty() {
        ui.label(
            RichText::new("Global")
                .size(8.0)
                .color(tc.text_secondary),
        );
        let mut to_remove = Vec::new();
        for id in &global_ids {
            if let Some(remove) = draw_binding_row(ui, bus, id, info, &targets) {
                if remove {
                    to_remove.push(id.clone());
                }
            }
        }
        for id in to_remove {
            bus.remove_binding(&id);
        }
    }

    if bus.bindings.is_empty() {
        ui.label(
            RichText::new("No bindings configured")
                .size(9.0)
                .color(tc.text_secondary),
        );
    }
}

/// A target option for the dropdown picker.
struct TargetOption {
    /// The target string (e.g. "param.Phosphor.warp_intensity").
    id: String,
    /// Display label (e.g. "warp_intensity").
    label: String,
    /// Group label (e.g. "Params", "Layers", "Global").
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
        targets.push(TargetOption {
            id: format!("layer.{i}.opacity"),
            label: format!("Layer {i} opacity"),
            group: "Layers",
        });
        targets.push(TargetOption {
            id: format!("layer.{i}.blend"),
            label: format!("Layer {i} blend"),
            group: "Layers",
        });
        targets.push(TargetOption {
            id: format!("layer.{i}.enabled"),
            label: format!("Layer {i} enabled"),
            group: "Layers",
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

/// Draw a single binding row. Returns Some(true) if should be deleted.
fn draw_binding_row(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    info: &BindingPanelInfo,
    targets: &[TargetOption],
) -> Option<bool> {
    let tc = theme_colors(ui.ctx());

    // Get binding data (clone to avoid borrow issues)
    let binding = bus.get_binding(id)?.clone();
    let runtime = bus.runtime(id);
    let last_output = runtime.and_then(|r| r.last_output);

    let expanded_id = egui::Id::new(format!("bind_expand_{}", id));
    let mut expanded = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(expanded_id).unwrap_or(false));

    let mut should_remove = false;

    // Collapsed row
    let display_name = make_display_name(&binding.source, &binding.target);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // Enable dot
        let dot_color = if binding.enabled {
            source_color(&binding.source)
        } else {
            Color32::from_rgb(0x33, 0x33, 0x33)
        };
        let (dot_rect, _) =
            ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(dot_rect.center(), 3.0, dot_color);

        // Source badge
        let src_short = abbreviate_source(&binding.source);
        ui.label(
            RichText::new(&src_short)
                .size(8.0)
                .color(source_color(&binding.source)),
        );

        // Name (clickable to expand): show custom name or auto-derived name
        let name_label = if binding.name.is_empty() {
            &display_name
        } else {
            &binding.name
        };
        if ui
            .add(
                egui::Label::new(RichText::new(name_label).size(9.0).color(tc.text_primary))
                    .sense(egui::Sense::click()),
            )
            .clicked()
        {
            expanded = !expanded;
        }

        // Right side
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Delete button
            if ui
                .add(
                    egui::Button::new(RichText::new("×").size(9.0))
                        .min_size(egui::vec2(14.0, 14.0)),
                )
                .on_hover_text("Remove binding")
                .clicked()
            {
                should_remove = true;
            }

            // Value bar (mini)
            if let Some(val) = last_output {
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::vec2(30.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    bar_rect,
                    1.0,
                    Color32::from_rgb(0x22, 0x22, 0x22),
                );
                let filled = egui::Rect::from_min_size(
                    bar_rect.min,
                    egui::vec2(bar_rect.width() * val.clamp(0.0, 1.0), bar_rect.height()),
                );
                ui.painter().rect_filled(
                    filled,
                    1.0,
                    source_color(&binding.source).linear_multiply(0.7),
                );
            }
        });
    });

    // Expanded details
    if expanded {
        ui.indent(format!("bind_detail_{id}"), |ui| {
            draw_binding_details(ui, bus, id, info, targets);
        });
    }

    ui.ctx().data_mut(|d| d.insert_temp(expanded_id, expanded));

    Some(should_remove)
}

/// Draw expanded binding details (source, target, transforms, etc.).
fn draw_binding_details(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    _info: &BindingPanelInfo,
    targets: &[TargetOption],
) {
    let tc = theme_colors(ui.ctx());

    let Some(binding) = bus.get_binding(id) else {
        return;
    };
    let mut source = binding.source.clone();
    let mut target = binding.target.clone();
    let mut name = binding.name.clone();
    let mut enabled = binding.enabled;
    let mut scope = binding.scope.clone();
    let transforms = binding.transforms.clone();

    // Diagnostics
    if let Some(runtime) = bus.runtime(id) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(input) = runtime.last_input {
                ui.label(
                    RichText::new(format!("in: {:.2}", input))
                        .size(8.0)
                        .color(tc.text_secondary),
                );
            }
            if let Some(output) = runtime.last_output {
                ui.label(
                    RichText::new(format!("out: {:.2}", output))
                        .size(8.0)
                        .color(tc.text_secondary),
                );
            }
            if let Some(ref raw) = runtime.last_raw {
                ui.label(
                    RichText::new(format!("raw: {}", raw.display))
                        .size(8.0)
                        .color(tc.text_secondary),
                );
            }
        });
    }

    // Source field + Learn button
    ui.horizontal(|ui| {
        ui.label(RichText::new("Source").size(8.0).color(tc.text_secondary));
        ui.add(
            egui::TextEdit::singleline(&mut source)
                .desired_width(140.0)
                .font(egui::TextStyle::Small),
        );

        // Learn button
        let is_learning = bus
            .learn_target
            .as_ref()
            .map_or(false, |l| l.binding_id == id && l.field == LearnField::Source);
        if is_learning {
            let t = ui.input(|i| i.time) as f32;
            let alpha = ((t * 4.0).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
            let color =
                Color32::from_rgba_unmultiplied(0xE0, 0xA0, 0x40, (alpha * 255.0) as u8);
            if ui
                .add(egui::Button::new(RichText::new("..").color(color).size(8.0)))
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

    // Target picker (dropdown)
    ui.horizontal(|ui| {
        ui.label(RichText::new("Target").size(8.0).color(tc.text_secondary));

        let current_label = target_display_label(&target, targets);
        let combo_resp = egui::ComboBox::from_id_salt(format!("target_pick_{id}"))
            .selected_text(RichText::new(&current_label).size(9.0))
            .width(180.0)
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

        // Also allow typing raw target for advanced use
        if combo_resp.response.secondary_clicked() {
            // Could open a text field — for now the combo is enough
        }
    });

    // Name field (editable, but show auto-name as placeholder)
    let auto_name = make_display_name(&source, &target);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Name").size(8.0).color(tc.text_secondary));
        let resp = ui.add(
            egui::TextEdit::singleline(&mut name)
                .desired_width(140.0)
                .font(egui::TextStyle::Small)
                .hint_text(&auto_name),
        );
        // Clear button to revert to auto-name
        if !name.is_empty()
            && ui
                .add(egui::Button::new(RichText::new("×").size(8.0)).min_size(egui::vec2(12.0, 12.0)))
                .on_hover_text("Use auto-generated name")
                .clicked()
        {
            name.clear();
        }
        let _ = resp;
    });

    // Enabled + Scope
    ui.horizontal(|ui| {
        ui.checkbox(&mut enabled, RichText::new("Enabled").size(8.0));
        ui.add_space(8.0);
        let scope_label = match scope {
            BindingScope::Preset => "Preset",
            BindingScope::Global => "Global",
        };
        if ui
            .add(egui::Button::new(RichText::new(scope_label).size(8.0)))
            .on_hover_text("Toggle scope")
            .clicked()
        {
            scope = match scope {
                BindingScope::Preset => BindingScope::Global,
                BindingScope::Global => BindingScope::Preset,
            };
        }
    });

    // Transforms
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Transforms")
                .size(8.0)
                .color(tc.text_secondary),
        );
    });
    draw_transform_editor(ui, bus, id, &transforms);

    // Apply changes back
    if let Some(b) = bus.get_binding_mut(id) {
        b.source = source;
        b.target = target;
        b.name = name;
        b.enabled = enabled;
        b.scope = scope;
    }

    ui.add_space(4.0);
}

/// Draw the transform tag editor.
fn draw_transform_editor(
    ui: &mut Ui,
    bus: &mut BindingBus,
    id: &str,
    transforms: &[TransformDef],
) {
    let tc = theme_colors(ui.ctx());

    // Show existing transforms as tags
    let mut to_remove: Option<usize> = None;
    for (i, t) in transforms.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let label = transform_label(t);
            ui.label(RichText::new(&label).size(8.0).color(tc.text_primary));

            if ui
                .add(
                    egui::Button::new(RichText::new("×").size(8.0))
                        .min_size(egui::vec2(12.0, 12.0)),
                )
                .on_hover_text("Remove transform")
                .clicked()
            {
                to_remove = Some(i);
            }
        });
    }

    if let Some(idx) = to_remove {
        if let Some(b) = bus.get_binding_mut(id) {
            b.transforms.remove(idx);
        }
    }

    // Add transform dropdown
    egui::ComboBox::from_id_salt(format!("add_xform_{id}"))
        .selected_text(RichText::new("+ transform").size(8.0))
        .width(100.0)
        .show_ui(ui, |ui| {
            let items = [
                (
                    "Remap",
                    TransformDef::Remap {
                        in_lo: 0.0,
                        in_hi: 1.0,
                        out_lo: 0.0,
                        out_hi: 1.0,
                    },
                ),
                ("Smooth", TransformDef::Smooth { factor: 0.8 }),
                ("Invert", TransformDef::Invert),
                ("Quantize", TransformDef::Quantize { steps: 4 }),
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
                ("Gate", TransformDef::Gate { threshold: 0.5 }),
                ("Scale", TransformDef::Scale { factor: 1.0 }),
                ("Offset", TransformDef::Offset { value: 0.0 }),
                (
                    "Clamp",
                    TransformDef::Clamp {
                        lo: 0.0,
                        hi: 1.0,
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

/// Get a short display label for a transform.
fn transform_label(t: &TransformDef) -> String {
    match t {
        TransformDef::Remap {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        } => {
            format!("remap [{in_lo:.1}–{in_hi:.1}]→[{out_lo:.1}–{out_hi:.1}]")
        }
        TransformDef::Smooth { factor } => format!("smooth({factor:.2})"),
        TransformDef::Invert => "invert".into(),
        TransformDef::Quantize { steps } => format!("quantize({steps})"),
        TransformDef::Deadzone { lo, hi } => format!("deadzone[{lo:.2}–{hi:.2}]"),
        TransformDef::Curve { curve_type } => format!("curve({curve_type})"),
        TransformDef::Gate { threshold } => format!("gate({threshold:.2})"),
        TransformDef::Scale { factor } => format!("scale({factor:.2})"),
        TransformDef::Offset { value } => format!("offset({value:.2})"),
        TransformDef::Clamp { lo, hi } => format!("clamp[{lo:.2}–{hi:.2}]"),
    }
}

/// Generate a human-readable display name from source + target strings.
fn make_display_name(source: &str, target: &str) -> String {
    let src = friendly_source(source);
    let tgt = friendly_target(target);
    if src.is_empty() && tgt.is_empty() {
        "(new binding)".into()
    } else if src.is_empty() {
        format!("? → {tgt}")
    } else if tgt.is_empty() {
        format!("{src} → ?")
    } else {
        format!("{src} → {tgt}")
    }
}

/// Make a source string human-readable.
/// "midi.MPD218_Port_A.cc.0.3" -> "CC 3"
/// "audio.kick" -> "kick"
/// "osc./phosphor/param/foo" -> "/foo"
fn friendly_source(source: &str) -> String {
    if source.is_empty() {
        return String::new();
    }
    if source.starts_with("midi.") {
        // midi.{device}.cc.{ch}.{cc} -> "CC {cc}"
        // midi.{device}.note.{ch}.{note} -> "Note {note}"
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
        return source.strip_prefix("midi.").unwrap_or(source).to_string();
    }
    if source.starts_with("audio.") {
        return source.strip_prefix("audio.").unwrap_or(source).to_string();
    }
    if source.starts_with("osc.") {
        // Show last path component
        let addr = source.strip_prefix("osc.").unwrap_or(source);
        return addr
            .rsplit('/')
            .next()
            .unwrap_or(addr)
            .to_string();
    }
    if source.starts_with("ws.") {
        let rest = source.strip_prefix("ws.").unwrap_or(source);
        return rest
            .rsplit('.')
            .next()
            .unwrap_or(rest)
            .to_string();
    }
    source.to_string()
}

/// Make a target string human-readable.
/// "param.Phosphor.warp_intensity" -> "warp_intensity"
/// "layer.0.opacity" -> "L0 opacity"
/// "global.master_opacity" -> "master opacity"
fn friendly_target(target: &str) -> String {
    if target.is_empty() {
        return String::new();
    }
    let parts: Vec<&str> = target.split('.').collect();
    match parts.first().copied() {
        Some("param") => {
            parts.get(2).unwrap_or(&"?").to_string()
        }
        Some("layer") => {
            let idx = parts.get(1).unwrap_or(&"?");
            let field = parts.get(2).unwrap_or(&"?");
            format!("L{idx} {field}")
        }
        Some("global") => {
            let field = parts.get(1).unwrap_or(&"?");
            field.replace('_', " ")
        }
        _ => target.to_string(),
    }
}

/// Look up a display label for a target string from the options list.
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

/// Color for a source type prefix.
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

/// Abbreviate a source string for compact display.
fn abbreviate_source(source: &str) -> String {
    if source.is_empty() {
        return "(none)".into();
    }
    friendly_source(source)
}

/// Draw a source type dot with label.
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
