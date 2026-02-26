pub mod colors;
pub mod dark;
pub mod light;
pub mod tokens;

use egui::Visuals;
use serde::{Deserialize, Serialize};

use colors::ThemeColors;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    HighContrast,
    Deuteranopia,
    Protanopia,
    Tritanopia,
}

impl ThemeMode {
    pub const ALL: &[ThemeMode] = &[
        ThemeMode::Dark,
        ThemeMode::Light,
        ThemeMode::HighContrast,
        ThemeMode::Deuteranopia,
        ThemeMode::Protanopia,
        ThemeMode::Tritanopia,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
            ThemeMode::HighContrast => "High Contrast",
            ThemeMode::Deuteranopia => "Deuteranopia",
            ThemeMode::Protanopia => "Protanopia",
            ThemeMode::Tritanopia => "Tritanopia",
        }
    }

    pub fn visuals(&self) -> Visuals {
        match self {
            ThemeMode::Dark | ThemeMode::Deuteranopia | ThemeMode::Protanopia | ThemeMode::Tritanopia => {
                dark::dark_visuals()
            }
            ThemeMode::Light => light::light_visuals(),
            ThemeMode::HighContrast => high_contrast_visuals(),
        }
    }

    pub fn colors(&self) -> ThemeColors {
        match self {
            ThemeMode::Dark => ThemeColors::dark(),
            ThemeMode::Light => ThemeColors::light(),
            ThemeMode::HighContrast => ThemeColors::high_contrast(),
            ThemeMode::Deuteranopia => ThemeColors::deuteranopia(),
            ThemeMode::Protanopia => ThemeColors::protanopia(),
            ThemeMode::Tritanopia => ThemeColors::tritanopia(),
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
            _ => ThemeMode::Dark,
        }
    }
}

fn high_contrast_visuals() -> Visuals {
    use egui::{Color32, Stroke};

    let mut v = dark::dark_visuals();

    // Override for maximum contrast
    v.panel_fill = Color32::from_rgb(0x0A, 0x0A, 0x0A);
    v.window_fill = Color32::from_rgb(0x0A, 0x0A, 0x0A);
    v.extreme_bg_color = Color32::BLACK;
    v.override_text_color = Some(Color32::WHITE);

    v.widgets.noninteractive.bg_fill = Color32::from_rgb(0x11, 0x11, 0x11);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(0xCC, 0xCC, 0xCC));
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x66, 0x66, 0x66));

    v.widgets.inactive.bg_fill = Color32::from_rgb(0x1A, 0x1A, 0x1A);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x66, 0x66, 0x66));

    v.widgets.hovered.bg_fill = Color32::from_rgb(0x28, 0x28, 0x28);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.hovered.bg_stroke = Stroke::new(2.0, Color32::from_rgb(0x55, 0xAA, 0xFF));

    v.widgets.active.bg_fill = Color32::from_rgb(0x33, 0x33, 0x33);
    v.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.active.bg_stroke = Stroke::new(2.0, Color32::from_rgb(0x55, 0xAA, 0xFF));

    v.selection.bg_fill = Color32::from_rgb(0x55, 0xAA, 0xFF).gamma_multiply(0.5);
    v.selection.stroke = Stroke::new(2.0, Color32::from_rgb(0x55, 0xAA, 0xFF));

    v.window_stroke = Stroke::new(2.0, Color32::from_rgb(0x66, 0x66, 0x66));

    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_mode_all_count() {
        assert_eq!(ThemeMode::ALL.len(), 6);
    }

    #[test]
    fn theme_mode_display_names() {
        for mode in ThemeMode::ALL {
            assert!(!mode.display_name().is_empty());
        }
    }

    #[test]
    fn theme_mode_toggle() {
        assert_eq!(ThemeMode::Dark.toggle(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.toggle(), ThemeMode::Dark);
        assert_eq!(ThemeMode::HighContrast.toggle(), ThemeMode::Dark);
    }

    #[test]
    fn theme_mode_default() {
        assert_eq!(ThemeMode::default(), ThemeMode::Dark);
    }

    #[test]
    fn theme_mode_serde_roundtrip() {
        for mode in ThemeMode::ALL {
            let json = serde_json::to_string(mode).unwrap();
            let m2: ThemeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(*mode, m2);
        }
    }
}
