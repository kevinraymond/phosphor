pub mod dark;
pub mod light;
pub mod tokens;

use egui::Visuals;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn detect_system() -> Self {
        match dark_light::detect() {
            Ok(dark_light::Mode::Light) => ThemeMode::Light,
            _ => ThemeMode::Dark,
        }
    }

    pub fn visuals(&self) -> Visuals {
        match self {
            ThemeMode::Dark => dark::dark_visuals(),
            ThemeMode::Light => light::light_visuals(),
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }
}
