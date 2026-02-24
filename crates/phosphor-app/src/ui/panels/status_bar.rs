use egui::{Color32, Ui};

use crate::gpu::ShaderUniforms;
use crate::ui::accessibility::keyboard::Shortcuts;

pub fn draw_status_bar(
    ui: &mut Ui,
    shader_error: &Option<String>,
    uniforms: &ShaderUniforms,
) {
    ui.horizontal(|ui| {
        // FPS
        let fps = if uniforms.delta_time > 0.0 {
            (1.0 / uniforms.delta_time) as u32
        } else {
            0
        };
        ui.label(format!("{fps} FPS"));

        ui.separator();

        // Shader status
        if let Some(err) = shader_error {
            ui.colored_label(
                Color32::from_rgb(0xE0, 0x60, 0x60),
                format!("Shader error: {err}"),
            );
        } else {
            ui.colored_label(
                Color32::from_rgb(0x50, 0xC0, 0x70),
                "Shader OK",
            );
        }

        ui.separator();

        // Hotkey legend (compact)
        ui.label("D:UI  F:Fullscreen  Esc:Quit");
    });
}
