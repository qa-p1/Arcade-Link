//! The kit applied to egui (feature `design-egui`): Visuals for both themes,
//! spacing, corner radii, the type scale and Inter.
//!
//! ```ignore
//! arcade_link::design::egui::apply(&ctx, "system", "arcade.lens");
//! ```

use super::{palette, radius, size, text, Palette, Theme};
use ::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use ::egui::{Color32, CornerRadius, FontData, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, ThemePreference, Vec2};

/// A token colour as an opaque egui colour.
pub fn colour(c: u32) -> Color32 {
    let [r, g, b] = super::rgb(c);
    Color32::from_rgb(r, g, b)
}

fn corner(r: f32) -> CornerRadius {
    CornerRadius::same(r as u8)
}

/// egui Visuals for one theme with `app_id`'s accent.
pub fn visuals(theme: Theme, app_id: &str) -> ::egui::Visuals {
    let p: Palette = palette(theme, app_id);
    let ink = super::ink(theme, app_id);
    let mut v = if theme.is_dark() { ::egui::Visuals::dark() } else { ::egui::Visuals::light() };
    let line = Stroke::new(1.0_f32, colour(p.line));
    let control = Stroke::new(1.0_f32, colour(p.line_strong));
    let widget = |fill: u32, stroke: Stroke, fg: u32| ::egui::style::WidgetVisuals {
        bg_fill: colour(fill),
        weak_bg_fill: colour(fill),
        bg_stroke: stroke,
        corner_radius: corner(radius::SM),
        fg_stroke: Stroke::new(1.5_f32, colour(fg)),
        expansion: 0.0,
    };
    v.widgets.noninteractive = widget(p.panel, line, p.text);
    v.widgets.inactive = widget(p.panel, control, p.text);
    v.widgets.hovered = widget(p.hover, control, p.text);
    v.widgets.active = widget(p.hover, Stroke::new(1.0_f32, colour(p.accent)), p.text);
    v.widgets.open = widget(p.sunken, control, p.text);
    v.selection.bg_fill = colour(p.accent_soft);
    v.selection.stroke = Stroke::new(1.0_f32, colour(p.accent));
    v.hyperlink_color = colour(ink.accent);
    v.faint_bg_color = colour(p.sunken);
    v.extreme_bg_color = colour(p.panel);
    v.text_edit_bg_color = Some(colour(p.panel));
    v.code_bg_color = colour(p.sunken);
    v.warn_fg_color = colour(ink.warn);
    v.error_fg_color = colour(ink.danger);
    v.window_fill = colour(p.panel);
    v.panel_fill = colour(p.bg);
    v.window_stroke = line;
    v.window_corner_radius = corner(radius::LG);
    v.menu_corner_radius = corner(radius::MD);
    let pop = if theme.is_dark() {
        Shadow { offset: [0, 16], blur: 48, spread: 0, color: Color32::from_black_alpha(0x88) }
    } else {
        Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_rgba_unmultiplied(0x10, 0x14, 0x1a, 0x24) }
    };
    v.window_shadow = pop;
    v.popup_shadow = pop;
    v.text_cursor.stroke = Stroke::new(2.0_f32, colour(p.accent));
    v.override_text_color = Some(colour(p.text));
    v.weak_text_color = Some(colour(p.muted));
    v.striped = false;
    v
}

/// Spacing, radii and the type scale.
pub fn style(theme: Theme, app_id: &str) -> ::egui::Style {
    let mut s = ::egui::Style { visuals: visuals(theme, app_id), ..::egui::Style::default() };
    s.spacing.item_spacing = Vec2::new(8.0, 6.0);
    s.spacing.button_padding = Vec2::new(12.0, 6.0);
    s.spacing.interact_size = Vec2::new(40.0, size::CONTROL_SM);
    s.spacing.window_margin = Margin::same(16);
    s.spacing.menu_margin = Margin::same(4);
    s.spacing.indent = 16.0;
    s.spacing.icon_width = 16.0;
    s.spacing.icon_spacing = 8.0;
    s.animation_time = super::motion::FAST_MS as f32 / 1000.0;
    let sans = |t: super::TypeStyle| FontId::new(t.size, FontFamily::Proportional);
    s.text_styles = [
        (TextStyle::Heading, sans(text::TITLE)),
        (TextStyle::Body, sans(text::BODY)),
        (TextStyle::Button, sans(text::CONTROL)),
        (TextStyle::Small, sans(text::SMALL)),
        (TextStyle::Monospace, FontId::new(text::MONO.size, FontFamily::Monospace)),
    ]
    .into();
    s
}

/// Adds Inter ahead of the app's existing proportional fonts, which stay as
/// fallbacks for glyphs Inter lacks.
pub fn install_inter(ctx: &::egui::Context) {
    ctx.add_font(FontInsert::new(
        "Inter",
        FontData::from_static(super::inter_font_bytes()),
        vec![InsertFontFamily { family: FontFamily::Proportional, priority: FontPriority::Highest }],
    ));
}

/// Styles `ctx` for `app_id` in both themes and selects the Appearance
/// setting (`"system"`, `"light"` or `"dark"`). Call again when it changes.
pub fn apply(ctx: &::egui::Context, setting: &str, app_id: &str) {
    ctx.set_style_of(::egui::Theme::Light, style(Theme::Light, app_id));
    ctx.set_style_of(::egui::Theme::Dark, style(Theme::Dark, app_id));
    ctx.set_theme(match setting {
        "light" => ThemePreference::Light,
        "dark" => ThemePreference::Dark,
        _ => ThemePreference::System,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visuals_follow_the_palette() {
        let v = visuals(Theme::Dark, "arcade.lens");
        assert!(v.dark_mode);
        assert_eq!(v.selection.stroke.color, colour(0x2bb3a3));
        assert_eq!(v.panel_fill, colour(0x111317));
        let s = style(Theme::Light, "arcade.find");
        assert_eq!(s.text_styles[&TextStyle::Body].size, 13.0);
        assert!(!s.visuals.dark_mode);
    }

    #[test]
    fn apply_installs_both_themes() {
        let ctx = ::egui::Context::default();
        install_inter(&ctx);
        apply(&ctx, "dark", "arcade.find");
        assert_eq!(ctx.theme(), ::egui::Theme::Dark);
        assert_eq!(ctx.style().visuals.hyperlink_color, colour(0x84cc16));
    }
}
