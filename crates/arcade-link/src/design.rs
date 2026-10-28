//! The Arcade design kit for Rust UIs (feature `design`; STANDARD.md §1-4).
//!
//! Every value comes from `design/tokens.json` through `design/generate.py`
//! ([`tokens`] is generated). Softbuffer/tiny-skia surfaces use [`palette`] and
//! [`ICONS`]; egui apps call [`egui::apply`] (feature `design-egui`).
//!
//! ```
//! use arcade_link::design::{palette, Theme};
//! let p = palette(Theme::Dark, "arcade.find");
//! assert_eq!(p.accent, 0x84CC16);
//! ```

#[cfg(feature = "design-egui")]
pub mod egui;
pub mod format;
#[rustfmt::skip]
mod tokens;

pub use tokens::{motion, radius, size, text, ACCENTS, ICONS, SPACE};

/// [`egui::apply`] under the name the overhaul specs use.
#[cfg(feature = "design-egui")]
pub use self::egui::apply as apply_egui;

/// Light or dark: the resolved theme, after the app's Appearance setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    /// Resolves an Appearance setting (`"system"`, `"light"` or `"dark"`)
    /// against the system preference. Unknown values follow the system.
    pub fn resolve(setting: &str, system_dark: bool) -> Theme {
        match setting {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ if system_dark => Theme::Dark,
            _ => Theme::Light,
        }
    }

    pub fn is_dark(self) -> bool {
        self == Theme::Dark
    }
}

/// One theme's colours with one app's accent. Colours are `0xRRGGBB`, except
/// `scrim`, which is `0xRRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub sunken: u32,
    pub hover: u32,
    pub line: u32,
    pub line_strong: u32,
    pub text: u32,
    pub muted: u32,
    pub faint: u32,
    pub ok: u32,
    pub ok_soft: u32,
    pub warn: u32,
    pub warn_soft: u32,
    pub danger: u32,
    pub danger_soft: u32,
    pub scrim: u32,
    /// Switch knobs.
    pub knob: u32,
    /// Text on a filled status colour (the destructive button).
    pub on_fill: u32,
    pub accent: u32,
    pub accent_text: u32,
    pub accent_soft: u32,
}

/// The shared neutrals and status colours of one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Base {
    bg: u32,
    panel: u32,
    sunken: u32,
    hover: u32,
    line: u32,
    line_strong: u32,
    text: u32,
    muted: u32,
    faint: u32,
    knob: u32,
    on_fill: u32,
    scrim: u32,
    ok: u32,
    ok_soft: u32,
    warn: u32,
    warn_soft: u32,
    danger: u32,
    danger_soft: u32,
}

/// One app's accent in both themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    pub app_id: &'static str,
    pub light: u32,
    pub dark: u32,
    pub text_light: u32,
    pub text_dark: u32,
    pub soft_light: u32,
    pub soft_dark: u32,
}

/// A type-scale entry: size and line height in px, CSS weight, tracking in em.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeStyle {
    pub size: f32,
    pub line: f32,
    pub weight: u16,
    pub tracking: f32,
}

/// An icon on the 24×24 grid as flat path operations: `0 x y` move, `1 x y`
/// line, `2 x1 y1 x2 y2 x y` cubic, `3` close. Draw with a 1.5 stroke, round
/// caps and joins, or fill it when `filled`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Icon {
    pub name: &'static str,
    pub filled: bool,
    pub ops: &'static [f32],
}

/// A drawing command decoded from [`Icon::ops`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathOp {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

impl Icon {
    /// The icon's commands, scaled from the 24 grid by `scale`.
    pub fn path(&self, scale: f32) -> impl Iterator<Item = PathOp> + '_ {
        let ops = self.ops;
        let mut i = 0;
        std::iter::from_fn(move || {
            let op = *ops.get(i)?;
            let at = |n: usize| ops[i + n] * scale;
            let (step, out) = match op as u8 {
                0 => (3, PathOp::Move(at(1), at(2))),
                1 => (3, PathOp::Line(at(1), at(2))),
                2 => (7, PathOp::Cubic(at(1), at(2), at(3), at(4), at(5), at(6))),
                _ => (1, PathOp::Close),
            };
            i += step;
            Some(out)
        })
    }
}

/// The accent of `app_id` (Arcade Tools' green for an unknown id).
pub fn accent(app_id: &str) -> &'static Accent {
    ACCENTS.iter().find(|a| a.app_id == app_id).unwrap_or(&ACCENTS[0])
}

/// The colours of `theme` with `app_id`'s accent.
pub fn palette(theme: Theme, app_id: &str) -> Palette {
    let base = match theme {
        Theme::Light => tokens::LIGHT,
        Theme::Dark => tokens::DARK,
    };
    let a = accent(app_id);
    let (accent, accent_text, accent_soft) = match theme {
        Theme::Light => (a.light, a.text_light, a.soft_light),
        Theme::Dark => (a.dark, a.text_dark, a.soft_dark),
    };
    Palette {
        bg: base.bg,
        panel: base.panel,
        sunken: base.sunken,
        hover: base.hover,
        line: base.line,
        line_strong: base.line_strong,
        text: base.text,
        muted: base.muted,
        faint: base.faint,
        ok: base.ok,
        ok_soft: base.ok_soft,
        warn: base.warn,
        warn_soft: base.warn_soft,
        danger: base.danger,
        danger_soft: base.danger_soft,
        scrim: base.scrim,
        knob: base.knob,
        on_fill: base.on_fill,
        accent,
        accent_text,
        accent_soft,
    }
}

/// `0xRRGGBB` as `[r, g, b]`.
pub const fn rgb(colour: u32) -> [u8; 3] {
    [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8]
}

/// `0xRRGGBB` as opaque `[r, g, b, 255]`.
pub const fn rgba(colour: u32) -> [u8; 4] {
    let [r, g, b] = rgb(colour);
    [r, g, b, 255]
}

/// `0xRRGGBBAA` (the scrim) as `[r, g, b, a]`.
pub const fn rgba_with_alpha(colour: u32) -> [u8; 4] {
    [(colour >> 24) as u8, (colour >> 16) as u8, (colour >> 8) as u8, colour as u8]
}

/// An icon by name.
pub fn icon(name: &str) -> Option<&'static Icon> {
    ICONS.iter().find(|i| i.name == name)
}

/// Inter 4 (variable, roman), bundled under the SIL Open Font License
/// (`design/fonts/OFL.txt`).
pub fn inter_font_bytes() -> &'static [u8] {
    include_bytes!("../../../design/fonts/InterVariable.ttf")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(colour: u32) -> f64 {
        let channel = |v: u8| {
            let c = f64::from(v) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let [r, g, b] = rgb(colour);
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn every_accent_meets_the_contrast_rules() {
        for a in ACCENTS {
            assert!(contrast(a.light, 0xffffff) >= 4.5, "{}", a.app_id);
            assert!(contrast(a.dark, palette(Theme::Dark, a.app_id).bg) >= 7.0, "{}", a.app_id);
            assert!(contrast(a.text_light, a.light) >= 4.5, "{}", a.app_id);
            assert!(contrast(a.text_dark, a.dark) >= 4.5, "{}", a.app_id);
        }
    }

    #[test]
    fn palettes_switch_theme_and_accent() {
        let light = palette(Theme::Light, "arcade.shelf");
        let dark = palette(Theme::Dark, "arcade.shelf");
        assert_eq!((light.bg, dark.bg), (0xf6f7f9, 0x111317));
        assert_eq!((light.accent, dark.accent), (0xbf13d8, 0xe273f3));
        assert_eq!(palette(Theme::Light, "unknown").accent, accent("arcade.tools").light);
        assert_eq!(Theme::resolve("system", true), Theme::Dark);
        assert_eq!(Theme::resolve("light", true), Theme::Light);
    }

    #[test]
    fn icons_decode_and_stay_on_the_grid() {
        assert!(ICONS.len() >= 27);
        for icon in ICONS {
            let ops: Vec<_> = icon.path(1.0).collect();
            assert!(matches!(ops.first(), Some(PathOp::Move(..))), "{}", icon.name);
            for op in ops {
                let points: Vec<f32> = match op {
                    PathOp::Move(x, y) | PathOp::Line(x, y) => vec![x, y],
                    PathOp::Cubic(a, b, c, d, e, f) => vec![a, b, c, d, e, f],
                    PathOp::Close => vec![],
                };
                assert!(points.iter().all(|v| (-1.0..=25.0).contains(v)), "{} leaves the grid: {op:?}", icon.name);
            }
        }
        assert!(icon("settings").is_some() && icon("nope").is_none());
    }

    #[test]
    fn inter_is_a_truetype_font() {
        let font = inter_font_bytes();
        assert_eq!(&font[..4], &[0, 1, 0, 0]);
        assert!(font.len() > 500_000);
    }
}
