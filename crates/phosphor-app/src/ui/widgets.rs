use egui::{collapsing_header::CollapsingState, pos2, Color32, CornerRadius, Frame, Margin, RichText, Shape, Stroke, Ui};

use super::theme::colors::theme_colors;
use super::theme::tokens::*;

/// Styled card frame for panel sections.
pub fn card_frame(ui: &Ui) -> Frame {
    let tc = theme_colors(ui.ctx());
    Frame {
        fill: tc.card_bg,
        stroke: Stroke::new(1.0, tc.card_border),
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
    let tc = theme_colors(ui.ctx());
    let id = ui.make_persistent_id(id);
    let state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);

    card_frame(ui).show(ui, |ui| {
        let full_width = ui.available_width();

        // Header row — always full width
        let header_response = ui.horizontal(|ui| {
            ui.set_min_width(full_width);
            draw_section_arrow(ui, state.is_open());
            ui.label(
                RichText::new(title.to_uppercase())
                    .size(HEADING_SIZE)
                    .color(tc.text_secondary)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(badge_text) = badge {
                    ui.label(
                        RichText::new(badge_text)
                            .size(SMALL_SIZE)
                            .color(tc.accent),
                    );
                }
            });
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

/// Draw a solid triangle indicator for collapsible sections.
fn draw_section_arrow(ui: &mut Ui, is_open: bool) {
    let tc = theme_colors(ui.ctx());
    let size = HEADING_SIZE;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let c = rect.center();
    let half = size * 0.3;
    let points = if is_open {
        // Down-pointing triangle
        vec![
            pos2(c.x - half, c.y - half * 0.5),
            pos2(c.x + half, c.y - half * 0.5),
            pos2(c.x, c.y + half * 0.5),
        ]
    } else {
        // Right-pointing triangle
        vec![
            pos2(c.x - half * 0.5, c.y - half),
            pos2(c.x + half * 0.5, c.y),
            pos2(c.x - half * 0.5, c.y + half),
        ]
    };
    ui.painter().add(Shape::convex_polygon(
        points,
        tc.text_secondary,
        Stroke::NONE,
    ));
}

/// Badge label in accent color at small size.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).size(SMALL_SIZE).color(color));
}
