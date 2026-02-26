use egui::{Color32, RichText, Ui};

use crate::ndi::types::OutputResolution;
use crate::ui::theme::colors::theme_colors;
use crate::ui::theme::tokens::*;

const NDI_GREEN: Color32 = Color32::from_rgb(0x40, 0xC0, 0x40);

/// Snapshot of NDI state for UI (avoids passing &mut NdiSystem through draw_panels).
#[derive(Clone, Default)]
pub struct NdiInfo {
    pub enabled: bool,
    pub running: bool,
    pub ndi_available: bool,
    pub source_name: String,
    pub resolution: OutputResolution,
    pub frames_sent: u64,
    pub output_width: u32,
    pub output_height: u32,
    pub alpha_from_luma: bool,
}

pub fn draw_ndi_panel(ui: &mut Ui, info: &NdiInfo) {
    let tc = theme_colors(ui.ctx());

    if !info.ndi_available {
        ui.label(
            RichText::new("NDI runtime not found. Install NDI Tools from ndi.video")
                .size(SMALL_SIZE)
                .color(tc.text_secondary),
        );
        return;
    }

    // Enable + status on one row
    ui.horizontal(|ui| {
        let mut enabled = info.enabled;
        if ui
            .checkbox(&mut enabled, RichText::new("Enable NDI").size(SMALL_SIZE))
            .changed()
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("ndi_set_enabled"), enabled);
            });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Frame counter
            if info.running {
                ui.label(
                    RichText::new(format!("{} sent", info.frames_sent))
                        .size(SMALL_SIZE)
                        .color(tc.text_secondary),
                );
            }
            // Activity dot
            let color = if info.running {
                NDI_GREEN
            } else {
                Color32::from_rgb(0x33, 0x33, 0x33)
            };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, color);
        });
    });

    // Source name
    ui.horizontal(|ui| {
        ui.label(RichText::new("Source").size(SMALL_SIZE));
        let mut name = info.source_name.clone();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut name)
                .desired_width(140.0)
                .font(egui::FontId::proportional(SMALL_SIZE)),
        );
        if resp.lost_focus() && name != info.source_name {
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("ndi_source_name"), name);
            });
        }
    });

    // Resolution dropdown
    ui.horizontal(|ui| {
        ui.label(RichText::new("Resolution").size(SMALL_SIZE));
        let current = info.resolution;
        egui::ComboBox::from_id_salt("ndi_resolution")
            .selected_text(current.display_name())
            .width(120.0)
            .show_ui(ui, |ui| {
                for (i, &res) in OutputResolution::ALL.iter().enumerate() {
                    if ui
                        .selectable_label(current == res, res.display_name())
                        .clicked()
                    {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("ndi_resolution_change"), i as u8);
                        });
                    }
                }
            });
    });

    // Alpha from luma toggle
    ui.horizontal(|ui| {
        let mut alpha_luma = info.alpha_from_luma;
        if ui
            .checkbox(
                &mut alpha_luma,
                RichText::new("Alpha from brightness").size(SMALL_SIZE),
            )
            .changed()
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("ndi_alpha_from_luma"), alpha_luma);
            });
        }
    });

    // Show output dimensions when running
    if info.running && info.output_width > 0 {
        ui.label(
            RichText::new(format!("Output: {}x{}", info.output_width, info.output_height))
                .size(SMALL_SIZE)
                .color(tc.text_secondary),
        );
    }
}
