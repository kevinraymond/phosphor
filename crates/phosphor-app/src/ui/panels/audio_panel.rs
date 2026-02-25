use egui::{Color32, Rect, RichText, Ui, Vec2};

use crate::audio::AudioSystem;
use crate::gpu::ShaderUniforms;
use crate::ui::theme::tokens::*;

const BAND_LABELS: [&str; 7] = ["SB", "BS", "LM", "MD", "UM", "PR", "BR"];

const BAND_COLORS: [Color32; 7] = [
    Color32::from_rgb(0xFF, 0x44, 0x66), // sub bass - red-pink
    Color32::from_rgb(0xFF, 0x88, 0x44), // bass - orange
    Color32::from_rgb(0xFF, 0xCC, 0x44), // low mid - yellow
    Color32::from_rgb(0x44, 0xFF, 0x88), // mid - green
    Color32::from_rgb(0x44, 0xCC, 0xFF), // upper mid - cyan
    Color32::from_rgb(0x44, 0x88, 0xFF), // presence - blue
    Color32::from_rgb(0xAA, 0x66, 0xFF), // brilliance - purple
];

fn draw_vertical_meters(ui: &mut Ui, bands: &[f32; 7]) {
    let available_width = ui.available_width();
    let total_gaps = (bands.len() - 1) as f32 * METER_GAP;
    let bar_width = ((available_width - total_gaps) / bands.len() as f32).max(8.0);
    let height = METER_HEIGHT;

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(available_width, height),
        egui::Sense::hover(),
    );

    // Background
    ui.painter().rect_filled(rect, 2.0, METER_BG);

    // Grid lines at 25%, 50%, 75%
    let grid_color = Color32::from_rgb(0x2A, 0x2A, 0x2A);
    for frac in [0.25, 0.5, 0.75] {
        let y = rect.bottom() - height * frac;
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.5, grid_color),
        );
    }

    // Bars
    for (i, &value) in bands.iter().enumerate() {
        let x = rect.left() + i as f32 * (bar_width + METER_GAP);
        let fill_height = height * value.clamp(0.0, 1.0);
        let bar_rect = Rect::from_min_size(
            egui::pos2(x, rect.bottom() - fill_height),
            Vec2::new(bar_width, fill_height),
        );
        ui.painter().rect_filled(bar_rect, 1.0, BAND_COLORS[i]);
    }

    // Labels below
    let (label_rect, _) = ui.allocate_exact_size(
        Vec2::new(available_width, 12.0),
        egui::Sense::hover(),
    );
    for (i, label) in BAND_LABELS.iter().enumerate() {
        let x = label_rect.left() + i as f32 * (bar_width + METER_GAP) + bar_width * 0.5;
        let y = label_rect.center().y;
        ui.painter().text(
            egui::pos2(x, y),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(SMALL_SIZE),
            DARK_TEXT_SECONDARY,
        );
    }
}

fn draw_feature_grid(ui: &mut Ui, uniforms: &ShaderUniforms) {
    let features: [(&str, f32); 6] = [
        ("RMS", uniforms.rms),
        ("Kick", uniforms.kick),
        ("Onset", uniforms.onset),
        ("Flux", uniforms.flux),
        ("Cent", uniforms.centroid),
        ("Flat", uniforms.flatness),
    ];

    egui::Grid::new("audio_features")
        .num_columns(4)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            for (i, (name, value)) in features.iter().enumerate() {
                ui.label(RichText::new(*name).size(SMALL_SIZE).color(DARK_TEXT_SECONDARY));
                ui.label(RichText::new(format!("{:.2}", value)).size(SMALL_SIZE).monospace());
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
}

fn draw_bpm_display(ui: &mut Ui, uniforms: &ShaderUniforms) {
    let bpm = uniforms.bpm * 300.0;
    if bpm <= 1.0 {
        return;
    }
    ui.horizontal(|ui| {
        // Beat dot
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
        let color = if uniforms.beat > 0.5 {
            BEAT_COLOR
        } else {
            Color32::from_rgb(0x44, 0x44, 0x44)
        };
        ui.painter().circle_filled(dot_rect.center(), 4.0, color);

        ui.label(
            RichText::new(format!("{:.0} BPM", bpm))
                .size(BODY_SIZE)
                .strong()
                .color(if uniforms.beat > 0.5 { BEAT_COLOR } else { DARK_TEXT_PRIMARY }),
        );
    });
}

pub fn draw_audio_panel(ui: &mut Ui, audio: &AudioSystem, uniforms: &ShaderUniforms) {
    if !audio.active {
        ui.colored_label(DARK_ERROR, "No audio input");
        return;
    }

    // Vertical frequency bars
    let bands: [f32; 7] = [
        uniforms.sub_bass, uniforms.bass, uniforms.low_mid, uniforms.mid,
        uniforms.upper_mid, uniforms.presence, uniforms.brilliance,
    ];
    draw_vertical_meters(ui, &bands);

    ui.add_space(4.0);

    // Compact feature grid
    draw_feature_grid(ui, uniforms);

    ui.add_space(2.0);

    // BPM + beat dot
    draw_bpm_display(ui, uniforms);
}
