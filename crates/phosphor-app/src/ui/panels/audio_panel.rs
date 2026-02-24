use egui::{Color32, Ui, Vec2};

use crate::audio::AudioSystem;
use crate::gpu::ShaderUniforms;
use crate::ui::accessibility::focus::draw_focus_ring;

const FEATURE_NAMES: [&str; 12] = [
    "Bass", "Mid", "Treble", "RMS", "Phase", "Onset",
    "Centroid", "Flux", "Flatness", "Rolloff", "Bandwidth", "ZCR",
];

/// HSV-cycled colors for spectrum bars.
fn bar_color(index: usize) -> Color32 {
    let hue = (index as f32 / 12.0) * 360.0;
    let (r, g, b) = hsv_to_rgb(hue, 0.7, 0.85);
    Color32::from_rgb(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    )
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32 / 60) % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

pub fn draw_audio_panel(ui: &mut Ui, audio: &AudioSystem, uniforms: &ShaderUniforms) {
    ui.label(format!("Device: {}", audio.device_name));

    if !audio.active {
        ui.colored_label(Color32::from_rgb(0xE0, 0x60, 0x60), "No audio input");
        return;
    }

    ui.add_space(4.0);

    let features: [f32; 12] = [
        uniforms.bass,
        uniforms.mid,
        uniforms.treble,
        uniforms.rms,
        uniforms.phase,
        uniforms.onset,
        uniforms.centroid,
        uniforms.flux,
        uniforms.flatness,
        uniforms.rolloff,
        uniforms.bandwidth,
        uniforms.zcr,
    ];

    let available_width = ui.available_width();
    let bar_height = 16.0;

    for (i, (name, &value)) in FEATURE_NAMES.iter().zip(features.iter()).enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{name:>9}"));
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(available_width - 120.0, bar_height),
                egui::Sense::hover(),
            );

            // Background
            ui.painter().rect_filled(
                rect,
                2.0,
                Color32::from_rgb(0x2A, 0x2A, 0x2A),
            );

            // Filled bar
            let fill_width = rect.width() * value.clamp(0.0, 1.0);
            let fill_rect = egui::Rect::from_min_size(
                rect.min,
                Vec2::new(fill_width, rect.height()),
            );
            ui.painter().rect_filled(fill_rect, 2.0, bar_color(i));

            draw_focus_ring(ui, &response);

            ui.label(format!("{value:.2}"));
        });
    }
}
