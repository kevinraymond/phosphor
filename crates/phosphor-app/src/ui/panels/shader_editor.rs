use std::path::PathBuf;

use egui::{
    Color32, CornerRadius, Frame, Id, Key, Margin, Modifiers, Order, RichText, Stroke, Vec2,
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

use crate::ui::theme::ThemeMode;
use crate::ui::theme::colors::theme_colors;

/// State for the shader editor overlay.
#[derive(Default)]
pub struct ShaderEditorState {
    pub open: bool,
    pub minimized: bool,
    pub file_path: Option<PathBuf>,
    pub file_name: String,
    pub effect_name: String,
    pub code: String,
    pub disk_content: String,
    pub compile_error: Option<String>,
    pub new_effect_prompt: bool,
    pub new_effect_name: String,
}

impl ShaderEditorState {
    pub fn is_dirty(&self) -> bool {
        self.code != self.disk_content
    }

    pub fn open_file(&mut self, effect_name: &str, path: PathBuf, content: String) {
        self.open = true;
        self.minimized = false;
        self.file_name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        self.effect_name = effect_name.to_string();
        self.file_path = Some(path);
        self.code = content.clone();
        self.disk_content = content;
        self.compile_error = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.file_path = None;
        self.code.clear();
        self.disk_content.clear();
        self.compile_error = None;
    }
}

/// Build a WGSL syntax definition for the code editor.
fn wgsl_syntax() -> Syntax {
    Syntax::new("wgsl")
        .with_case_sensitive(true)
        .with_comment("//")
        .with_comment_multiline(["/*", "*/"])
        .with_keywords([
            "fn", "var", "let", "const", "struct", "return", "if", "else", "for", "while", "loop",
            "break", "continue", "discard", "switch", "case", "default", "override", "enable",
            "true", "false",
        ])
        .with_types([
            "f32", "f16", "i32", "u32", "bool", "vec2f", "vec3f", "vec4f", "vec2i", "vec3i",
            "vec4i", "vec2u", "vec3u", "vec4u", "mat2x2f", "mat3x3f", "mat4x4f", "array",
            "texture_2d", "texture_storage_2d", "sampler", "ptr",
        ])
        .with_special([
            "@fragment",
            "@vertex",
            "@compute",
            "@group",
            "@binding",
            "@location",
            "@builtin",
            "@workgroup_size",
            "uniform",
            "storage",
            "read",
            "read_write",
        ])
}

/// Map the app theme to a code editor ColorTheme, with transparent background.
fn editor_color_theme(theme: ThemeMode) -> ColorTheme {
    let mut ct = match theme {
        ThemeMode::Light => ColorTheme::GITHUB_LIGHT,
        _ => ColorTheme::AYU_DARK,
    };
    // Set bg to fully transparent (hex #00000000)
    ct.bg = "#00000000";
    ct
}

/// Draw the shader editor as an overlay with transparent code area.
/// Returns true if the editor is open.
pub fn draw_shader_editor(ctx: &egui::Context, state: &mut ShaderEditorState, theme: ThemeMode) -> bool {
    if !state.open {
        return false;
    }

    let tc = theme_colors(ctx);
    let screen = ctx.input(|i| i.screen_rect());

    // Header bar height + toolbar + separators
    let header_height = 60.0;
    let error_height = if state.compile_error.is_some() { 28.0 } else { 0.0 };

    // Minimized: header + 5 lines (~100px). Maximized: ~85% of screen.
    let code_height = if state.minimized {
        5.0 * 17.0 // ~5 lines at 13pt + line spacing
    } else {
        (screen.height() * 0.85).max(300.0) - header_height - error_height
    };
    let panel_h = header_height + code_height + error_height;
    let panel_w = (screen.width() * 0.85).max(500.0).min(screen.width() - 20.0);

    // Position: centered horizontally, anchored to bottom (with small margin)
    let panel_pos = egui::pos2(
        (screen.width() - panel_w) * 0.5,
        screen.height() - panel_h - 30.0, // 30px from bottom for status bar
    );

    // Opaque header fill
    let header_fill = tc.panel;

    let editor_id = Id::new("shader_editor_overlay");
    egui::Area::new(editor_id)
        .fixed_pos(panel_pos)
        .order(Order::Foreground)
        .show(ctx, |ui| {
            ui.set_min_width(panel_w);
            ui.set_max_width(panel_w);

            // --- Opaque header frame ---
            Frame {
                fill: header_fill,
                inner_margin: Margin::same(0),
                stroke: Stroke::new(1.0, tc.card_border),
                corner_radius: CornerRadius {
                    nw: 8,
                    ne: 8,
                    sw: if state.minimized && state.compile_error.is_none() { 8 } else { 0 },
                    se: if state.minimized && state.compile_error.is_none() { 8 } else { 0 },
                },
                ..Default::default()
            }
            .show(ui, |ui| {
                // Title bar
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(&state.effect_name)
                            .size(14.0)
                            .color(tc.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(&state.file_name)
                            .size(12.0)
                            .color(tc.text_secondary),
                    );
                    if state.is_dirty() {
                        ui.label(RichText::new("*").size(14.0).color(tc.warning));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        // Close button
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{2715}").size(14.0).color(tc.text_secondary),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                            )
                            .on_hover_text("Close (Esc)")
                            .clicked()
                        {
                            state.close();
                        }

                        // Minimize/maximize toggle
                        let toggle_label = if state.minimized { "\u{25B3}" } else { "\u{25BD}" };
                        let toggle_tip = if state.minimized { "Expand" } else { "Minimize" };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(toggle_label).size(14.0).color(tc.text_secondary),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                            )
                            .on_hover_text(toggle_tip)
                            .clicked()
                        {
                            state.minimized = !state.minimized;
                        }
                    });
                });

                ui.add_space(2.0);
                ui.separator();

                // Toolbar: Save / Revert
                ui.horizontal(|ui| {
                    ui.add_space(10.0);

                    let save_enabled = state.is_dirty();
                    let save_btn = ui.add_enabled(
                        save_enabled,
                        egui::Button::new(
                            RichText::new("Save")
                                .size(12.0)
                                .color(if save_enabled { tc.text_primary } else { tc.text_secondary }),
                        )
                        .fill(tc.card_bg)
                        .stroke(Stroke::new(1.0, tc.card_border))
                        .corner_radius(CornerRadius::same(3)),
                    );
                    if save_btn.clicked() {
                        ctx.data_mut(|d| {
                            d.insert_temp(Id::new("shader_editor_save"), true);
                        });
                    }
                    save_btn.on_hover_text("Ctrl+S");

                    if ui
                        .add_enabled(
                            state.is_dirty(),
                            egui::Button::new(
                                RichText::new("Revert").size(12.0).color(tc.text_secondary),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                        )
                        .on_hover_text("Discard changes")
                        .clicked()
                    {
                        state.code = state.disk_content.clone();
                    }
                });
                ui.add_space(2.0);
            });

            // --- Transparent code area ---
            if !state.minimized || state.compile_error.is_some() {
                Frame {
                    fill: Color32::TRANSPARENT,
                    inner_margin: Margin::same(0),
                    stroke: Stroke::new(1.0, tc.card_border),
                    corner_radius: CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 8,
                        se: 8,
                    },
                    ..Default::default()
                }
                .show(ui, |ui| {
                    if !state.minimized {
                        // Code editor
                        let syntax = wgsl_syntax();
                        let color_theme = editor_color_theme(theme);

                        egui::ScrollArea::vertical()
                            .max_height(code_height)
                            .show(ui, |ui| {
                                CodeEditor::default()
                                    .id_source("shader_code_editor")
                                    .with_rows(if state.minimized { 5 } else { 60 })
                                    .with_fontsize(13.0)
                                    .with_theme(color_theme)
                                    .with_syntax(syntax)
                                    .with_numlines(true)
                                    .vscroll(false)
                                    .desired_width(f32::INFINITY)
                                    .show(ui, &mut state.code);
                            });
                    }

                    // Compile error bar
                    if let Some(ref error) = state.compile_error {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(truncate_error(error, 200))
                                    .size(11.0)
                                    .color(tc.error),
                            );
                        });
                    }
                });
            }
        });

    // Handle Ctrl+S (extract key check first to avoid nested ctx locks)
    let ctrl_s = ctx.input(|i| {
        i.key_pressed(Key::S) && i.modifiers.matches_exact(Modifiers::COMMAND)
    });
    if ctrl_s && state.is_dirty() {
        ctx.data_mut(|d| {
            d.insert_temp(Id::new("shader_editor_save"), true);
        });
    }

    // Handle Esc to close
    let esc = ctx.input(|i| i.key_pressed(Key::Escape));
    if esc {
        state.close();
    }

    true
}

/// Draw the "New Effect" name prompt as a small centered window.
pub fn draw_new_effect_prompt(ctx: &egui::Context, state: &mut ShaderEditorState) {
    if !state.new_effect_prompt {
        return;
    }

    let tc = theme_colors(ctx);

    egui::Window::new("New Effect")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size(Vec2::new(300.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Effect name:")
                    .size(13.0)
                    .color(tc.text_primary),
            );
            ui.add_space(4.0);
            let response = ui.text_edit_singleline(&mut state.new_effect_name);

            // Auto-focus
            if response.gained_focus() || state.new_effect_name.is_empty() {
                response.request_focus();
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let name_valid = !state.new_effect_name.trim().is_empty();
                let enter_pressed = ui.input(|i| i.key_pressed(Key::Enter));

                if ui
                    .add_enabled(
                        name_valid,
                        egui::Button::new("Create"),
                    )
                    .clicked()
                    || (enter_pressed && name_valid)
                {
                    let name = state.new_effect_name.trim().to_string();
                    ctx.data_mut(|d| {
                        d.insert_temp(Id::new("create_new_effect"), name);
                    });
                    state.new_effect_prompt = false;
                    state.new_effect_name.clear();
                }

                if ui.button("Cancel").clicked()
                    || ui.input(|i| i.key_pressed(Key::Escape))
                {
                    state.new_effect_prompt = false;
                    state.new_effect_name.clear();
                }
            });
        });
}

fn truncate_error(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}
