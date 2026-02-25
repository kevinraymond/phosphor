use egui::{collapsing_header::CollapsingState, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use super::theme::tokens::*;

/// Styled card frame for panel sections.
pub fn card_frame() -> Frame {
    Frame {
        fill: CARD_BG,
        stroke: Stroke::new(1.0, CARD_BORDER),
        corner_radius: CornerRadius::same(CARD_ROUNDING),
        inner_margin: Margin::same(CARD_PADDING as i8),
        outer_margin: Margin::symmetric(0, CARD_MARGIN as i8),
        ..Default::default()
    }
}

/// Collapsible section with card styling.
/// Returns the inner `Ui` response if the section is open.
pub fn section(
    ui: &mut Ui,
    id: &str,
    title: &str,
    badge: Option<&str>,
    default_open: bool,
    add_body: impl FnOnce(&mut Ui),
) {
    let id = ui.make_persistent_id(id);
    let state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);

    card_frame().show(ui, |ui| {
        // Header row
        let header_response = ui.horizontal(|ui| {
            let arrow = if state.is_open() { "\u{25BE}" } else { "\u{25B8}" }; // ▾ / ▸
            ui.label(RichText::new(arrow).size(HEADING_SIZE).color(DARK_TEXT_SECONDARY));
            ui.label(
                RichText::new(title.to_uppercase())
                    .size(HEADING_SIZE)
                    .color(DARK_TEXT_SECONDARY)
                    .strong(),
            );
            if let Some(badge_text) = badge {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(badge_text)
                            .size(SMALL_SIZE)
                            .color(DARK_ACCENT),
                    );
                });
            }
        });

        // Toggle on header click
        if header_response.response.interact(egui::Sense::click()).clicked() {
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
            state.toggle(ui);
            state.store(ui.ctx());
        }

        // Body
        if state.is_open() {
            ui.add_space(4.0);
            add_body(ui);
        }
    });
}

/// Badge label in accent color at small size.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).size(SMALL_SIZE).color(color));
}
