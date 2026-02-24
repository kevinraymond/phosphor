use egui::Ui;

use crate::effect::EffectLoader;

pub fn draw_effect_panel(ui: &mut Ui, loader: &EffectLoader) {
    if loader.effects.is_empty() {
        ui.label("No effects found in assets/effects/");
        return;
    }

    for (i, effect) in loader.effects.iter().enumerate() {
        let is_current = loader.current_effect == Some(i);
        let text = if is_current {
            format!("> {}", effect.name)
        } else {
            effect.name.clone()
        };

        let response = ui.selectable_label(is_current, &text);
        if response.clicked() && !is_current {
            // Signal to main loop to load this effect
            // This is handled by checking pending_effect_load in the event loop
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("pending_effect"), i));
        }

        if !effect.description.is_empty() {
            response.on_hover_text(&effect.description);
        }
    }
}
