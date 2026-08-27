use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};

pub const BACKGROUND: Color32 = Color32::from_rgb(12, 17, 24);
pub const PANEL: Color32 = Color32::from_rgb(18, 25, 35);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(25, 34, 47);
pub const PANEL_HOVER: Color32 = Color32::from_rgb(32, 44, 60);
pub const BORDER: Color32 = Color32::from_rgb(47, 61, 80);
pub const TEXT: Color32 = Color32::from_rgb(232, 238, 247);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(145, 160, 180);
pub const ACCENT: Color32 = Color32::from_rgb(91, 167, 255);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(32, 80, 132);
pub const SUCCESS: Color32 = Color32::from_rgb(73, 199, 128);
pub const WARNING: Color32 = Color32::from_rgb(242, 183, 76);
pub const DANGER: Color32 = Color32::from_rgb(239, 112, 126);

pub fn apply(context: &egui::Context) {
    let mut style = (*context.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(40.0, 34.0);
    style.spacing.window_margin = egui::Margin::same(18);
    style.visuals = visuals();
    context.set_style(style);
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(TEXT_MUTED);
    visuals.hyperlink_color = ACCENT;
    visuals.faint_bg_color = PANEL;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.text_edit_bg_color = Some(BACKGROUND);
    visuals.code_bg_color = BACKGROUND;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = DANGER;
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.window_fill = PANEL;
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.panel_fill = BACKGROUND;
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);

    visuals.widgets.inactive.bg_fill = PANEL_RAISED;
    visuals.widgets.inactive.weak_bg_fill = PANEL_RAISED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);

    visuals.widgets.hovered.bg_fill = PANEL_HOVER;
    visuals.widgets.hovered.weak_bg_fill = PANEL_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);

    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);

    visuals.widgets.open = visuals.widgets.active;
    visuals
}
