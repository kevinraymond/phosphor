use super::bus::BindingBus;
use super::types::{BindingScope, TransformDef};

/// A built-in binding template.
#[derive(Debug)]
pub struct BindingTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub entries: &'static [TemplateEntry],
}

/// A single entry in a template.
#[derive(Debug)]
pub struct TemplateEntry {
    pub source: &'static str,
    /// Target pattern. Three placeholders are substituted by [`BindingBus::apply_template`]:
    ///
    /// * `{layer}` — the index of the layer the template is applied to.
    /// * `{effect}` — that layer's effect name.
    /// * `{param_N}` — the Nth param of that effect, positionally. Unresolved → entry skipped.
    /// * `{param: a|b|*}` — the first of `a`, `b`, … the effect actually has, else (for `*`)
    ///   the next param no other entry in this template has claimed. Unresolved → skipped.
    pub target_pattern: &'static str,
    pub transforms: fn() -> Vec<TransformDef>,
    pub scope: BindingScope,
}

/// Params the `{param: …|*}` fallback must not grab.
///
/// Sweeping one of these re-seeds or flattens the effect rather than modulating it: a
/// kick-driven `trail_decay` strobes the whole feedback buffer, and the mode/preset-style
/// selectors snap between discrete looks instead of moving. They stay reachable *by name* —
/// `centroid → color_mode` is exactly what drift.pfx's own `audio_mappings` asks for.
/// Ignored entirely when nothing else is free, because a jarring binding still beats a dead one.
const FALLBACK_SKIP: &[&str] = &[
    "trail_decay",
    "trail_length",
    "color_mode",
    "mode",
    "preset",
    "init_pattern",
    "attractor",
    "projection",
    "layout",
    "bass_mode",
    "dipole_mode",
    "num_species",
    "symmetry",
    "feed_rate",
    "kill_rate",
];

/// "The param that reads as impact" — kick and beat.
const T_PUNCH: &str = "param.{layer}.{effect}.{param: warp_intensity|audio_drive\
|audio_reactivity|shard_force|burst_power|burst_force|flash_power|field_strength|beat_pulse\
|intensity|flow_intensity|spring_k|*}";

/// "The param that changes the colour" — centroid.
const T_COLOUR: &str = "param.{layer}.{effect}.{param: color_shift|color_mode|hue|ice_hue\
|water_hue|saturation|brightness|exposure|edge_glow|emitter_glow|glow_width|glow|*}";

/// "The param that turns or advances" — beat phase.
const T_MOTION: &str = "param.{layer}.{effect}.{param: rotation|rotation_speed|field_rotation\
|orbit_speed|swirl|twist_amount|drift_speed|curtain_speed|expansion_speed|turn_speed\
|growth_speed|sim_speed|speed_mult|inward_speed|flow_speed|dt_scale|camera_pitch|ribbon_drift\
|speed|*}";

/// All available built-in templates.
pub fn builtin_templates() -> &'static [&'static BindingTemplate] {
    static ALL: &[&BindingTemplate] = &[&AUDIO_REACTIVE, &BEAT_SYNC, &SPECTRAL_BANDS, &MIDI_FADERS];
    ALL
}

static AUDIO_REACTIVE: BindingTemplate = BindingTemplate {
    name: "Audio Reactive",
    description: "Kick, RMS, centroid, and beat phase mapped to common params",
    entries: &[
        TemplateEntry {
            source: "audio.kick",
            target_pattern: T_PUNCH,
            transforms: || vec![TransformDef::Smooth { factor: 0.7 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.rms",
            target_pattern: "layer.{layer}.opacity",
            transforms: || {
                vec![
                    TransformDef::Remap {
                        in_lo: 0.0,
                        in_hi: 0.8,
                        out_lo: 0.3,
                        out_hi: 1.0,
                    },
                    TransformDef::Smooth { factor: 0.85 },
                ]
            },
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.centroid",
            target_pattern: T_COLOUR,
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.beat_phase",
            target_pattern: T_MOTION,
            transforms: || vec![],
            scope: BindingScope::Preset,
        },
    ],
};

static BEAT_SYNC: BindingTemplate = BindingTemplate {
    name: "Beat Sync",
    description: "Beat gate and smooth phase for rhythmic control",
    entries: &[
        TemplateEntry {
            source: "audio.beat",
            target_pattern: T_PUNCH,
            transforms: || {
                vec![
                    TransformDef::Gate { threshold: 0.5 },
                    TransformDef::Smooth { factor: 0.6 },
                ]
            },
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.beat_phase",
            target_pattern: T_MOTION,
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
    ],
};

static SPECTRAL_BANDS: BindingTemplate = BindingTemplate {
    name: "Spectral Bands",
    description: "7 frequency bands mapped to first 7 params",
    entries: &[
        TemplateEntry {
            source: "audio.band.0",
            target_pattern: "param.{layer}.{effect}.{param_0}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.band.1",
            target_pattern: "param.{layer}.{effect}.{param_1}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.band.2",
            target_pattern: "param.{layer}.{effect}.{param_2}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.band.3",
            target_pattern: "param.{layer}.{effect}.{param_3}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.band.4",
            target_pattern: "param.{layer}.{effect}.{param_4}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.band.5",
            target_pattern: "param.{layer}.{effect}.{param_5}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "audio.band.6",
            target_pattern: "param.{layer}.{effect}.{param_6}",
            transforms: || vec![TransformDef::Smooth { factor: 0.8 }],
            scope: BindingScope::Preset,
        },
    ],
};

static MIDI_FADERS: BindingTemplate = BindingTemplate {
    name: "MIDI Faders",
    description: "CC 1-8 mapped to first 8 params",
    entries: &[
        TemplateEntry {
            source: "midi.*.cc.0.1",
            target_pattern: "param.{layer}.{effect}.{param_0}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.2",
            target_pattern: "param.{layer}.{effect}.{param_1}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.3",
            target_pattern: "param.{layer}.{effect}.{param_2}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.4",
            target_pattern: "param.{layer}.{effect}.{param_3}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.5",
            target_pattern: "param.{layer}.{effect}.{param_4}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.6",
            target_pattern: "param.{layer}.{effect}.{param_5}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.7",
            target_pattern: "param.{layer}.{effect}.{param_6}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
        TemplateEntry {
            source: "midi.*.cc.0.8",
            target_pattern: "param.{layer}.{effect}.{param_7}",
            transforms: || vec![TransformDef::Smooth { factor: 0.9 }],
            scope: BindingScope::Preset,
        },
    ],
};

/// Byte span of a `{param: …}` placeholder, if the pattern has one.
fn param_span(pattern: &str) -> Option<(usize, usize)> {
    let open = pattern.find("{param:")?;
    let close = pattern[open..].find('}')? + open;
    Some((open, close))
}

/// Resolve every `{param: a|b|*}` in a template against one effect's params, returning
/// the chosen name per entry (`None` = drop that entry).
///
/// Two passes, and the order matters: a single left-to-right pass lets an earlier
/// entry's `*` grab a param a later entry names outright. On Tunnel that put centroid
/// on `speed` and left beat_phase — which names `speed` — with `tunnel_radius`.
///
/// Nothing is ever claimed twice, which is what stops three sources piling onto a
/// one-param effect's single slider, where they would fight and last write would win.
fn resolve_template_params(
    template: &BindingTemplate,
    param_names: &[String],
) -> Vec<Option<String>> {
    let bodies: Vec<Option<&str>> = template
        .entries
        .iter()
        .map(|e| param_span(e.target_pattern).map(|(o, c)| &e.target_pattern[o + 7..c]))
        .collect();
    let mut out: Vec<Option<String>> = vec![None; template.entries.len()];
    let mut claimed: Vec<String> = Vec::new();

    // Pass 1 — named candidates only, in each entry's own preference order.
    for (i, body) in bodies.iter().enumerate() {
        let Some(body) = body else { continue };
        out[i] = body
            .split('|')
            .map(str::trim)
            .filter(|c| *c != "*")
            .find(|c| param_names.iter().any(|n| n == c) && !claimed.iter().any(|n| n == c))
            .map(|c| {
                claimed.push(c.to_string());
                c.to_string()
            });
    }

    // Pass 2 — wildcards take whatever is left, preferring a param that is safe to
    // sweep but taking a skipped one rather than resolving to nothing.
    for (i, body) in bodies.iter().enumerate() {
        let Some(body) = body else { continue };
        if out[i].is_some() || !body.split('|').any(|c| c.trim() == "*") {
            continue;
        }
        let free = || param_names.iter().filter(|n| !claimed.contains(n));
        let pick = free()
            .find(|n| !FALLBACK_SKIP.contains(&n.as_str()))
            .or_else(|| free().next())
            .cloned();
        if let Some(name) = pick {
            claimed.push(name.clone());
            out[i] = Some(name);
        }
    }
    out
}

impl BindingBus {
    /// Apply a template to `layer_idx`, creating bindings with resolved target patterns.
    ///
    /// Emits the 4-part `param.{layer}.{effect}.{name}` target — the form
    /// `build_target_options` produces and `apply_binding_target` resolves without
    /// consulting the active layer. The old 3-part `param.{effect}.{name}` worked only
    /// while the layer it was applied to happened to still be selected; pick another
    /// layer and every template binding went quietly dead. Reading 3-part is unchanged,
    /// so saved bindings keep working.
    pub fn apply_template(
        &mut self,
        template: &BindingTemplate,
        layer_idx: usize,
        effect_name: &str,
        param_names: &[String],
    ) {
        // Media and webcam layers report an empty effect name; substituting it would
        // emit the unresolvable target `param.0..warp_intensity`.
        if effect_name.is_empty() {
            return;
        }

        // Substitute a live MIDI device for `*` source placeholders: evaluation
        // is exact-match on snapshot keys, so a literal `midi.*.cc.…` binding
        // can never fire. If no MIDI source has been seen yet, the `*` stays —
        // the card then shows the no-signal chip until the user re-Learns it.
        let midi_device: Option<String> = self
            .last_snapshot
            .keys()
            .find(|k| k.starts_with("midi."))
            .and_then(|k| k.split('.').nth(1))
            .map(str::to_string);

        // `{param: a|b|*}` — a preference list, not a hardcoded name. The old patterns
        // named warp_intensity / color_shift / rotation, which each exist on exactly one
        // of the 40 shipped effects, so "Audio Reactive" produced three dead bindings on
        // everything but Drift, Flux and Cymatics. Resolved for the whole template at
        // once so entries cannot claim the same param.
        let chosen = resolve_template_params(template, param_names);

        for (entry, choice) in template.entries.iter().zip(&chosen) {
            let mut target = entry
                .target_pattern
                .replace("{effect}", effect_name)
                .replace("{layer}", &layer_idx.to_string());

            if let (Some((open, close)), Some(name)) = (param_span(&target), choice) {
                target.replace_range(open..=close, name);
            }

            // Replace {param_N} placeholders with actual param names
            for (i, name) in param_names.iter().enumerate() {
                target = target.replace(&format!("{{param_{i}}}"), name);
            }

            // Skip if we still have unresolved placeholders. Broadened from "{param_":
            // `{param:` has no underscore, so the narrow check let an unresolved
            // candidate list through and emitted it as a literal target.
            if target.contains("{param") {
                continue;
            }

            let mut source = entry.source.to_string();
            if let (true, Some(dev)) = (source.contains('*'), midi_device.as_deref()) {
                source = source.replace('*', dev);
            }
            let id = self.add_binding(source, target, entry.scope.clone());

            // Apply transforms
            if let Some(b) = self.get_binding_mut(&id) {
                b.transforms = (entry.transforms)();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_bus() -> BindingBus {
        BindingBus {
            bindings: Vec::new(),
            runtimes: HashMap::new(),
            ws_bind_values: HashMap::new(),
            ws_preview_images: HashMap::new(),
            ws_field_last_seen: HashMap::new(),
            next_id_counter: 1,
            dirty: false,
            dirty_since: None,
            preset_scope_dirty: false,
            learn_target: None,
            last_snapshot: HashMap::new(),
            pending_triggers: Vec::new(),
        }
    }

    /// Param names of the given effect, straight from the shipped `.pfx` files.
    /// CARGO_MANIFEST_DIR, not assets_dir(): the latter resolves CWD-relative, and
    /// `cargo test` runs with CWD = crates/phosphor-app, which has no assets/.
    fn shipped_effects() -> Vec<(String, Vec<String>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/effects");
        let mut out: Vec<(String, Vec<String>)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "pfx"))
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .filter_map(|j| serde_json::from_str::<crate::effect::format::PfxEffect>(&j).ok())
            .map(|e| {
                let params = e
                    .inputs
                    .iter()
                    .filter(|d| {
                        matches!(
                            d,
                            crate::params::ParamDef::Float { .. }
                                | crate::params::ParamDef::Bool { .. }
                        )
                    })
                    .map(|d| d.name().to_string())
                    .collect();
                (e.name, params)
            })
            .collect();
        out.sort();
        assert!(!out.is_empty(), "no .pfx files found in {}", dir.display());
        out
    }

    #[test]
    fn audio_reactive_prefers_named_params() {
        let mut bus = test_bus();
        // Drift's real inputs.
        let params: Vec<String> = ["warp_intensity", "flow_speed", "color_mode", "density"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        bus.apply_template(&AUDIO_REACTIVE, 0, "Drift", &params);
        assert_eq!(bus.bindings.len(), 4);
        assert_eq!(bus.bindings[0].source, "audio.kick");
        assert_eq!(bus.bindings[0].target, "param.0.Drift.warp_intensity");
        assert_eq!(bus.bindings[1].target, "layer.0.opacity");
        // color_mode is in FALLBACK_SKIP but named in the colour list, so it still wins —
        // drift.pfx's own audio_mappings ask for exactly centroid -> palette colour.
        assert_eq!(bus.bindings[2].target, "param.0.Drift.color_mode");
        assert_eq!(bus.bindings[3].target, "param.0.Drift.flow_speed");
    }

    #[test]
    fn audio_reactive_falls_back_when_no_name_matches() {
        let mut bus = test_bus();
        // Strata shares no vocabulary with any candidate list.
        let params: Vec<String> = [
            "height_scale",
            "draw_distance",
            "camera_pitch",
            "rock_detail",
            "snow_line",
            "zoom",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        bus.apply_template(&AUDIO_REACTIVE, 0, "Strata", &params);
        assert_eq!(bus.bindings.len(), 4);
        let hit: Vec<&str> = bus
            .bindings
            .iter()
            .filter_map(|b| b.target.strip_prefix("param.0.Strata."))
            .collect();
        assert_eq!(hit.len(), 3, "every param entry should resolve: {hit:?}");
        let mut sorted = hit.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "a param was bound twice: {hit:?}");
    }

    #[test]
    fn audio_reactive_never_double_binds_one_param() {
        // Phosphor declares a single param. Without the `claimed` set, kick, centroid
        // and beat_phase would all land on it and fight, last write winning.
        let mut bus = test_bus();
        let params = vec!["trail_decay".to_string()];
        bus.apply_template(&AUDIO_REACTIVE, 0, "Phosphor", &params);
        assert_eq!(bus.bindings.len(), 2); // one param + layer opacity
        assert_eq!(bus.bindings[0].target, "param.0.Phosphor.trail_decay");
        assert_eq!(bus.bindings[1].target, "layer.0.opacity");
    }

    #[test]
    fn audio_reactive_on_a_paramless_effect_still_binds_opacity() {
        // The 8 Lattice rules and the hidden stress effect declare no inputs at all.
        let mut bus = test_bus();
        bus.apply_template(&AUDIO_REACTIVE, 2, "Lattice Clouds", &[]);
        assert_eq!(bus.bindings.len(), 1);
        assert_eq!(bus.bindings[0].target, "layer.2.opacity");
    }

    #[test]
    fn fallback_avoids_simulation_resetting_params() {
        // Sweeping trail_decay strobes the whole feedback buffer rather than modulating,
        // so the kick wildcard must reach past it.
        let mut bus = test_bus();
        let params = vec!["trail_decay".to_string(), "curl_scale".to_string()];
        bus.apply_template(&AUDIO_REACTIVE, 0, "X", &params);
        assert_eq!(bus.bindings[0].source, "audio.kick");
        assert_eq!(bus.bindings[0].target, "param.0.X.curl_scale");
    }

    #[test]
    fn fallback_uses_a_denied_param_rather_than_nothing() {
        let mut bus = test_bus();
        let params = vec!["trail_decay".to_string()];
        bus.apply_template(&AUDIO_REACTIVE, 0, "X", &params);
        assert_eq!(bus.bindings[0].target, "param.0.X.trail_decay");
    }

    #[test]
    fn a_wildcard_does_not_steal_a_param_a_later_entry_names() {
        // Tunnel names two motion params and nothing the colour list knows. A single
        // left-to-right pass let centroid's wildcard take `speed` first and left
        // beat_phase — the entry that actually names it — with `tunnel_radius`.
        let mut bus = test_bus();
        let params: Vec<String> = [
            "twist_amount",
            "speed",
            "tunnel_radius",
            "rib_density",
            "pinch_h",
            "pinch_v",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        bus.apply_template(&AUDIO_REACTIVE, 0, "Tunnel", &params);
        let phase = bus
            .bindings
            .iter()
            .find(|b| b.source == "audio.beat_phase")
            .expect("beat_phase entry should resolve");
        assert_eq!(phase.target, "param.0.Tunnel.twist_amount");
        // …and the wildcards then take what the named passes left.
        let centroid = bus
            .bindings
            .iter()
            .find(|b| b.source == "audio.centroid")
            .expect("centroid entry should resolve");
        assert_ne!(centroid.target, "param.0.Tunnel.twist_amount");
    }

    #[test]
    fn unresolved_candidate_list_is_skipped_not_emitted_literally() {
        // The trap: the skip guard tested for "{param_", and `{param:` has no
        // underscore, so an unresolved list would sail through as a literal target.
        let mut bus = test_bus();
        bus.apply_template(&AUDIO_REACTIVE, 0, "Nothing", &[]);
        assert!(
            !bus.bindings.iter().any(|b| b.target.contains('{')),
            "emitted an unresolved placeholder: {:?}",
            bus.bindings.iter().map(|b| &b.target).collect::<Vec<_>>()
        );
    }

    #[test]
    fn audio_reactive_moves_something_on_every_shipped_effect() {
        // The bug this replaces: the template's targets were only ever verified
        // against a hand-fed param list that no shipped effect actually has.
        for (name, params) in shipped_effects() {
            if params.is_empty() {
                continue; // 8 Lattice rules + the hidden stress effect
            }
            let mut bus = test_bus();
            bus.apply_template(&AUDIO_REACTIVE, 0, &name, &params);
            let hit: Vec<&str> = bus
                .bindings
                .iter()
                .filter_map(|b| b.target.strip_prefix("param.0."))
                .filter_map(|r| r.split_once('.'))
                .map(|(_, p)| p)
                .collect();
            assert!(!hit.is_empty(), "'{name}' gets no param binding at all");
            for p in &hit {
                assert!(
                    params.iter().any(|have| have == p),
                    "'{name}' bound to '{p}', which it does not have"
                );
            }
            let mut sorted = hit.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), hit.len(), "'{name}' double-bound a param");
        }
    }

    #[test]
    fn template_targets_the_layer_it_was_applied_to() {
        // Pre-fix this emitted param.{effect}.{name}, which apply_binding_target only
        // honours while that effect is on the *active* layer.
        let mut bus = test_bus();
        let params: Vec<String> = ["warp_intensity", "flow_speed", "color_mode"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        bus.apply_template(&AUDIO_REACTIVE, 3, "Drift", &params);
        for b in &bus.bindings {
            if b.target.starts_with("param.") {
                assert!(b.target.starts_with("param.3.Drift."), "{}", b.target);
            } else {
                assert_eq!(b.target, "layer.3.opacity");
            }
        }
    }

    #[test]
    fn template_is_a_noop_on_a_layer_with_no_effect() {
        // Media and webcam layers report an empty effect name.
        let mut bus = test_bus();
        bus.apply_template(&AUDIO_REACTIVE, 0, "", &[]);
        assert!(bus.bindings.is_empty());
    }

    #[test]
    fn spectral_skips_missing_params() {
        let mut bus = test_bus();
        let params = vec!["warp".into(), "color".into()]; // Only 2 params
        bus.apply_template(&SPECTRAL_BANDS, 0, "Phosphor", &params);
        // Should only create 2 bindings (bands 0 and 1), skip 2-6
        assert_eq!(bus.bindings.len(), 2);
        assert_eq!(bus.bindings[0].target, "param.0.Phosphor.warp");
    }

    #[test]
    fn midi_faders_substitutes_live_device() {
        use crate::bindings::types::SourceRaw;
        let mut bus = test_bus();
        bus.last_snapshot.insert(
            "midi.MPD218.cc.0.1".to_string(),
            (
                0.5,
                SourceRaw {
                    display: "0.5".into(),
                    numeric: 0.5,
                },
            ),
        );
        let params: Vec<String> = (0..8).map(|i| format!("p{i}")).collect();
        bus.apply_template(&MIDI_FADERS, 0, "Phosphor", &params);
        assert_eq!(bus.bindings.len(), 8);
        for (i, b) in bus.bindings.iter().enumerate() {
            assert_eq!(b.source, format!("midi.MPD218.cc.0.{}", i + 1));
        }
    }

    #[test]
    fn midi_faders_keeps_wildcard_without_live_device() {
        let mut bus = test_bus();
        let params: Vec<String> = (0..8).map(|i| format!("p{i}")).collect();
        bus.apply_template(&MIDI_FADERS, 0, "Phosphor", &params);
        // No live MIDI source: the placeholder survives so the UI can flag it.
        assert_eq!(bus.bindings[0].source, "midi.*.cc.0.1");
    }

    #[test]
    fn builtin_templates_available() {
        let templates = builtin_templates();
        assert_eq!(templates.len(), 4);
        assert_eq!(templates[0].name, "Audio Reactive");
        assert_eq!(templates[1].name, "Beat Sync");
        assert_eq!(templates[2].name, "Spectral Bands");
        assert_eq!(templates[3].name, "MIDI Faders");
    }
}
