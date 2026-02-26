use egui::Color32;

/// Runtime theme color set used by all UI panels.
/// Stored in egui temp data, read via `theme_colors(ctx)`.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub canvas: Color32,
    pub panel: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub success: Color32,
    pub widget_bg: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    pub beat_color: Color32,
    pub meter_bg: Color32,
    pub separator: Color32,
}

impl ThemeColors {
    pub fn dark() -> Self {
        Self {
            canvas: Color32::from_rgb(0x12, 0x12, 0x12),
            panel: Color32::from_rgba_premultiplied(0x22, 0x22, 0x22, 0xE6),
            text_primary: Color32::from_rgb(0xE8, 0xE8, 0xE8),
            text_secondary: Color32::from_rgb(0xA0, 0xA0, 0xA0),
            accent: Color32::from_rgb(0x44, 0x88, 0xFF),
            error: Color32::from_rgb(0xE0, 0x60, 0x60),
            warning: Color32::from_rgb(0xD4, 0xA0, 0x40),
            success: Color32::from_rgb(0x50, 0xC0, 0x70),
            widget_bg: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            card_bg: Color32::from_rgb(0x24, 0x24, 0x24),
            card_border: Color32::from_rgb(0x33, 0x33, 0x33),
            beat_color: Color32::from_rgb(0xFF, 0x55, 0x77),
            meter_bg: Color32::from_rgb(0x1A, 0x1A, 0x1A),
            separator: Color32::from_rgb(0x3A, 0x3A, 0x3A),
        }
    }

    pub fn light() -> Self {
        Self {
            canvas: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            text_primary: Color32::from_rgb(0x1A, 0x1A, 0x1A),
            text_secondary: Color32::from_rgb(0x5A, 0x5A, 0x5A),
            accent: Color32::from_rgb(0x09, 0x69, 0xA8),
            error: Color32::from_rgb(0xC0, 0x30, 0x30),
            warning: Color32::from_rgb(0xA0, 0x70, 0x10),
            success: Color32::from_rgb(0x20, 0x80, 0x40),
            widget_bg: Color32::from_rgb(0xE8, 0xE8, 0xE8),
            card_bg: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            card_border: Color32::from_rgb(0xD5, 0xD5, 0xD5),
            beat_color: Color32::from_rgb(0xD0, 0x30, 0x50),
            meter_bg: Color32::from_rgb(0xE0, 0xE0, 0xE0),
            separator: Color32::from_rgb(0xD5, 0xD5, 0xD5),
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            canvas: Color32::from_rgb(0x00, 0x00, 0x00),
            panel: Color32::from_rgb(0x0A, 0x0A, 0x0A),
            text_primary: Color32::WHITE,
            text_secondary: Color32::from_rgb(0xCC, 0xCC, 0xCC),
            accent: Color32::from_rgb(0x55, 0xAA, 0xFF),
            error: Color32::from_rgb(0xFF, 0x44, 0x44),
            warning: Color32::from_rgb(0xFF, 0xCC, 0x00),
            success: Color32::from_rgb(0x44, 0xFF, 0x44),
            widget_bg: Color32::from_rgb(0x1A, 0x1A, 0x1A),
            card_bg: Color32::from_rgb(0x11, 0x11, 0x11),
            card_border: Color32::from_rgb(0x66, 0x66, 0x66),
            beat_color: Color32::from_rgb(0xFF, 0x44, 0x88),
            meter_bg: Color32::from_rgb(0x08, 0x08, 0x08),
            separator: Color32::from_rgb(0x55, 0x55, 0x55),
        }
    }

    /// Deuteranopia (red-green colorblind): error=orange, success=blue, beat=orange-gold
    pub fn deuteranopia() -> Self {
        let mut tc = Self::dark();
        tc.error = Color32::from_rgb(0xE0, 0x80, 0x20); // orange
        tc.success = Color32::from_rgb(0x40, 0x90, 0xE0); // blue
        tc.beat_color = Color32::from_rgb(0xE0, 0xA0, 0x30); // orange-gold
        tc
    }

    /// Protanopia (red-blind): error=yellow-gold, success=blue, beat=gold
    pub fn protanopia() -> Self {
        let mut tc = Self::dark();
        tc.error = Color32::from_rgb(0xD0, 0xB0, 0x20); // yellow-gold
        tc.success = Color32::from_rgb(0x40, 0x90, 0xE0); // blue
        tc.beat_color = Color32::from_rgb(0xD0, 0xA0, 0x20); // gold
        tc
    }

    /// Tritanopia (blue-yellow): accent=red, warning=cyan, success=cyan
    pub fn tritanopia() -> Self {
        let mut tc = Self::dark();
        tc.accent = Color32::from_rgb(0xE0, 0x50, 0x50); // red
        tc.warning = Color32::from_rgb(0x40, 0xC0, 0xC0); // cyan
        tc.success = Color32::from_rgb(0x40, 0xC0, 0xC0); // cyan
        tc.beat_color = Color32::from_rgb(0xFF, 0x70, 0x70); // bright red
        tc
    }
}

const THEME_COLORS_ID: &str = "phosphor_theme_colors";

/// Store theme colors in egui temp data.
pub fn set_theme_colors(ctx: &egui::Context, colors: ThemeColors) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(THEME_COLORS_ID), colors));
}

/// Read theme colors from egui temp data (fallback: dark).
pub fn theme_colors(ctx: &egui::Context) -> ThemeColors {
    ctx.data(|d| d.get_temp(egui::Id::new(THEME_COLORS_ID)))
        .unwrap_or_else(ThemeColors::dark)
}
