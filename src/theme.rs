//! The dark theme used by the editor.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x15, 0x16, 0x1b);
pub const EDITOR_BG: Color32 = Color32::from_rgb(0x1c, 0x1e, 0x24);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1f);
pub const STATUS_BG: Color32 = Color32::from_rgb(0x12, 0x13, 0x17);
pub const INPUT_BG: Color32 = Color32::from_rgb(0x22, 0x24, 0x2c);
pub const HOVER_BG: Color32 = Color32::from_rgb(0x2a, 0x2d, 0x36);
pub const ACTIVE_BG: Color32 = Color32::from_rgb(0x33, 0x37, 0x42);
pub const BORDER: Color32 = Color32::from_rgb(0x2b, 0x2e, 0x38);
pub const TEXT: Color32 = Color32::from_rgb(0xcd, 0xd1, 0xdb);
pub const MUTED: Color32 = Color32::from_rgb(0x72, 0x78, 0x87);
pub const FAINT: Color32 = Color32::from_rgb(0x4b, 0x50, 0x5d);
pub const ACCENT: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xf7);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x3d, 0x59, 0x8c);
pub const SELECTION: Color32 = Color32::from_rgb(0x2c, 0x44, 0x6e);
pub const SCROLL_HANDLE: Color32 = Color32::from_rgb(0x56, 0x5b, 0x68);
pub const SCROLL_HANDLE_HOVER: Color32 = Color32::from_rgb(0x7a, 0x81, 0x92);
pub const GUIDE: Color32 = Color32::from_rgba_premultiplied(0x55, 0x5b, 0x6b, 0x90);
pub const MATCH_BG: Color32 = Color32::from_rgba_premultiplied(0x3a, 0x50, 0x7a, 0x90);
pub const MATCH_CURRENT_BG: Color32 = Color32::from_rgba_premultiplied(0x41, 0x5f, 0x9e, 0xb4);
pub const ERROR: Color32 = Color32::from_rgb(0xf0, 0x76, 0x76);
pub const MODIFIED: Color32 = Color32::from_rgb(0xe0, 0xaf, 0x68);

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(19.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Monospace),
        ),
    ]
    .into();

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL_BG;
    visuals.extreme_bg_color = INPUT_BG;
    visuals.faint_bg_color = PANEL_BG;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    visuals.selection.bg_fill = SELECTION;
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    visuals.text_cursor.stroke = Stroke::new(1.5, ACCENT);

    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = INPUT_BG;
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = HOVER_BG;
    visuals.widgets.hovered.weak_bg_fill = HOVER_BG;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);

    visuals.widgets.active.bg_fill = ACTIVE_BG;
    visuals.widgets.active.weak_bg_fill = ACTIVE_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);

    visuals.widgets.open.bg_fill = INPUT_BG;
    visuals.widgets.open.weak_bg_fill = INPUT_BG;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::same(6);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.interact_size.y = 24.0;
    style.spacing.scroll.floating = false;
    style.spacing.scroll.bar_width = 9.0;
    style.spacing.scroll.bar_inner_margin = 5.0;
    style.spacing.scroll.bar_outer_margin = 2.0;
    style.animation_time = 0.12;

    ctx.set_style_of(egui::Theme::Dark, style);
}
