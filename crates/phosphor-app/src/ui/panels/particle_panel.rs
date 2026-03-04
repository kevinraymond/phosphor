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

    ui.add_space(6.0);

    // Editable params (communicated back via egui temp data)
    let mut emit_rate = info.emit_rate;
    let mut burst = info.burst_on_beat;
    let mut lifetime = info.lifetime;
    let mut speed = info.initial_speed;
    let mut size = info.initial_size;
    let mut drag = info.drag;

    egui::Grid::new("particle_params")
        .num_columns(2)
        .spacing([8.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Emit rate").size(SMALL_SIZE).color(tc.text_secondary));
            let r = ui.add(
                egui::Slider::new(&mut emit_rate, 10.0..=5000.0)
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
                egui::Slider::new(&mut burst, 0..=2000)
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
                egui::Slider::new(&mut lifetime, 0.5..=30.0)
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
                egui::Slider::new(&mut speed, 0.01..=2.0)
                    .logarithmic(true)
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
                egui::Slider::new(&mut size, 0.001..=0.1)
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
                egui::Slider::new(&mut drag, 0.8..=1.0)
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
