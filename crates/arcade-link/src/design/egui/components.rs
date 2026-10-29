//! Shared egui controls for Arcade windows and overlays (STANDARD §5–7).
//!
//! Call [`super::install_inter`] once, then [`super::apply`] when Appearance
//! changes. Construct a [`Kit`] inside each frame so System follows egui's live
//! native theme. Apps supply data and actions; the kit owns layout and styling.

use super::colour;
use crate::design::{ink, motion, palette, radius, size, text, Ink, Palette, PathOp, Theme};
use ::egui::{
    Align, Align2, Color32, Context, Frame, Id, InnerResponse, Key, Label, Layout, Margin, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextEdit, Ui,
    Vec2, WidgetInfo, WidgetType,
};
use std::time::{Duration, Instant};

/// A component's semantic foreground and soft background.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Neutral,
    Ok,
    Warn,
    Danger,
    Accent,
}

/// Shared button variants. Small and icon-only keep a 28-point hit target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonKind {
    #[default]
    Default,
    Primary,
    Ghost,
    Danger,
    Destructive,
}

/// A sidebar item; `icon` is a name from the kit icon table.
pub struct NavItem<'a> {
    pub label: &'a str,
    pub icon: &'a str,
    pub count: Option<usize>,
}

/// State of the shared shortcut recorder. The app supplies a conflict lookup
/// covering Link's registry and its desktop's registered bindings.
#[derive(Default)]
pub struct ShortcutRecorder {
    pub recording: bool,
    pending: Option<(String, String)>,
    error: Option<String>,
}

impl ShortcutRecorder {
    pub fn show(&mut self, ui: &mut Ui, kit: &Kit, value: &mut String, default: &str, allow_clear: bool, conflict: impl Fn(&str) -> Option<String>) -> bool {
        let mut changed = false;
        let mut candidate = None;
        ui.horizontal_wrapped(|ui| {
            if self.recording {
                let r = kit.button(ui, "Press keys…", ButtonKind::Default);
                r.request_focus();
                let event = ui.input(|input| {
                    input.events.iter().find_map(|event| match event {
                        ::egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } => Some((*key, *modifiers)),
                        _ => None,
                    })
                });
                if let Some((key, modifiers)) = event {
                    if key == Key::Escape {
                        self.recording = false;
                    } else if key == Key::Backspace && allow_clear {
                        candidate = Some(String::new());
                    } else {
                        match recorded_accelerator(key, modifiers) {
                            Ok(value) => candidate = Some(value),
                            Err(message) => self.error = Some(message.into()),
                        }
                    }
                }
            } else {
                let platform = if cfg!(target_os = "macos") {
                    crate::accelerator::Platform::Macos
                } else if cfg!(windows) {
                    crate::accelerator::Platform::Windows
                } else {
                    crate::accelerator::Platform::Linux
                };
                let display = crate::accelerator::display(value, platform).unwrap_or_else(|_| value.clone());
                for key in display.split('+') {
                    kit.key_chip(ui, if key.is_empty() { "None" } else { key });
                }
                if kit.button(ui, "Change…", ButtonKind::Default).clicked() {
                    self.recording = true;
                    self.pending = None;
                    self.error = None;
                }
            }
            if kit.button(ui, "Reset to default", ButtonKind::Ghost).clicked() {
                candidate = Some(default.to_string());
            }
        });
        if let Some(candidate) = candidate {
            self.error = None;
            self.recording = false;
            if let Some(owner) = (!candidate.is_empty()).then(|| conflict(&candidate)).flatten() {
                self.pending = Some((candidate, owner));
            } else {
                changed = *value != candidate;
                *value = candidate;
                self.pending = None;
            }
        }
        if let Some((candidate, owner)) = self.pending.clone() {
            kit.notice_action(ui, &format!("Used by {owner}"), Tone::Warn, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if kit.button(ui, "Use anyway", ButtonKind::Default).clicked() {
                        changed = *value != candidate;
                        *value = candidate;
                        self.pending = None;
                    }
                    if kit.button(ui, "Choose another", ButtonKind::Ghost).clicked() {
                        self.pending = None;
                        self.recording = true;
                    }
                });
            });
        }
        if let Some(error) = &self.error {
            kit.notice(ui, error, Tone::Warn);
        }
        changed
    }
}

fn recorded_accelerator(key: Key, modifiers: ::egui::Modifiers) -> Result<String, &'static str> {
    let name = match key {
        Key::ArrowDown => "Down",
        Key::ArrowUp => "Up",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        _ => key.name(),
    };
    let function_key = name.strip_prefix('F').and_then(|value| value.parse::<u8>().ok()).is_some_and(|number| (1..=24).contains(&number));
    if !function_key && !modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.mac_cmd && !modifiers.command {
        return Err("Include Ctrl, Alt, Shift or Super, or use a function key.");
    }
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.mac_cmd || (modifiers.command && !modifiers.ctrl) {
        parts.push("Super");
    }
    parts.push(name);
    crate::accelerator::normalize(&parts.join("+")).map_err(|_| "Couldn't use that shortcut. Choose another combination.")
}

/// Per-frame resolved tokens. The same API serves Find and Lens.
#[derive(Clone, Copy)]
pub struct Kit {
    pub palette: Palette,
    pub ink: Ink,
}

impl Kit {
    pub fn new(ui: &Ui, app_id: &str) -> Self {
        Self::for_theme(if ui.visuals().dark_mode { Theme::Dark } else { Theme::Light }, app_id)
    }

    pub fn for_theme(theme: Theme, app_id: &str) -> Self {
        Self { palette: palette(theme, app_id), ink: ink(theme, app_id) }
    }

    pub fn tones(&self, tone: Tone) -> (Color32, Color32) {
        let p = self.palette;
        let (fg, bg) = match tone {
            Tone::Neutral => (p.muted, p.sunken),
            Tone::Ok => (self.ink.ok, p.ok_soft),
            Tone::Warn => (self.ink.warn, p.warn_soft),
            Tone::Danger => (self.ink.danger, p.danger_soft),
            Tone::Accent => (self.ink.accent, p.accent_soft),
        };
        (colour(fg), colour(bg))
    }

    /// A standard button. Disabled/busy preserve width and accessible text.
    pub fn button(&self, ui: &mut Ui, label: &str, kind: ButtonKind) -> Response {
        self.button_state(ui, label, kind, true, false, false)
    }

    pub fn button_state(&self, ui: &mut Ui, label: &str, kind: ButtonKind, enabled: bool, busy: bool, small: bool) -> Response {
        let p = self.palette;
        let (mut fill, mut fg, mut border) = match kind {
            ButtonKind::Default => (p.panel, p.text, p.muted),
            ButtonKind::Primary => (p.accent, p.accent_text, p.accent),
            ButtonKind::Ghost => (p.bg, self.ink.accent, p.bg),
            ButtonKind::Danger => (p.panel, self.ink.danger, self.ink.danger),
            ButtonKind::Destructive => (p.danger, p.on_fill, p.danger),
        };
        let enabled = enabled && ui.is_enabled();
        if !enabled {
            fill = p.sunken;
            fg = p.muted;
            border = p.line_strong;
        }
        let font = super::font(text::CONTROL);
        let mut job = ::egui::text::LayoutJob::simple_singleline(label.to_string(), font, colour(fg));
        let natural_width = ui.fonts_mut(|fonts| fonts.layout_job(job.clone())).size().x;
        // Reserve the leading status slot even when idle: becoming busy never
        // changes the button width or its neighbors' positions.
        let width = (natural_width + 48.0).max(size::HIT_TARGET).min(ui.available_width());
        let height = if small { size::CONTROL_SM } else { size::CONTROL };
        job.wrap.max_width = (width - 40.0).max(0.0);
        job.wrap.max_rows = 1;
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        let r = ui.add_enabled_ui(enabled && !busy, |ui| ui.allocate_exact_size(Vec2::new(width, height), Sense::click()).1).inner;
        let hover_fill = match kind {
            ButtonKind::Primary | ButtonKind::Destructive => fill,
            ButtonKind::Danger => p.danger_soft,
            _ => p.hover,
        };
        let active = r.hovered() && enabled && !busy;
        let pressed = r.is_pointer_button_down_on();
        ui.painter().rect(
            r.rect,
            radius::SM,
            if kind == ButtonKind::Ghost && !active && enabled { Color32::TRANSPARENT } else { colour(if active { hover_fill } else { fill }) },
            Stroke::new(if pressed { 2.0 } else { 1.0 }, colour(border)),
            StrokeKind::Inside,
        );
        let text_pos = r.rect.center() - galley.size() * 0.5 + Vec2::new(8.0, 0.0);
        ui.painter().galley(text_pos, galley, colour(fg));
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled && !busy, label));
        self.focus(ui, &r);
        if busy {
            let rect = Rect::from_center_size(r.rect.left_center() + Vec2::new(16.0, 0.0), Vec2::splat(size::ICON_SMALL));
            ui.put(rect, ::egui::Spinner::new().size(size::ICON_SMALL).color(colour(fg)));
        }
        r.on_hover_text(label)
    }

    pub fn icon_button(&self, ui: &mut Ui, name: &str, label: &str) -> Response {
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(size::CONTROL_SM), Sense::click());
        let fill = if r.is_pointer_button_down_on() || r.hovered() { self.palette.hover } else { self.palette.panel };
        ui.painter().rect_filled(rect, radius::SM, colour(fill));
        self.paint_icon(ui, name, rect.center(), colour(self.palette.text));
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
        self.focus(ui, &r);
        r.on_hover_text(label)
    }

    pub fn page_head(&self, ui: &mut Ui, title: &str, description: &str) {
        ui.label(RichText::new(title).font(super::font(text::TITLE)).strong().color(colour(self.palette.text)));
        ui.add(Label::new(RichText::new(description).font(super::font(text::SMALL)).color(colour(self.palette.muted))).wrap());
        ui.add_space(24.0);
    }

    pub fn section_head(&self, ui: &mut Ui, title: &str) {
        ui.label(RichText::new(title).font(super::font(text::HEADING)).strong().color(colour(self.palette.text)));
        ui.add_space(10.0);
    }

    /// A bounded list panel; scrolling is owned by the app's page/panel.
    pub fn list<R>(&self, ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        Frame::new().fill(colour(self.palette.panel)).stroke(Stroke::new(1.0, colour(self.palette.line))).corner_radius(radius::MD).inner_margin(12.0).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width());
                contents(ui)
            },
        )
    }

    /// Label and optional description, with an app-supplied trailing control.
    /// On narrow pages the control moves below; it never steals the text width.
    pub fn row<R>(&self, ui: &mut Ui, title: &str, description: &str, control: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let width = ui.available_width();
        let narrow = width < 480.0 * ui.ctx().zoom_factor();
        let r = ui.vertical(|ui| {
            ui.set_min_height(size::ROW);
            if narrow {
                self.row_text(ui, title, description);
                control(ui)
            } else {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(Vec2::new((width * 0.52).max(160.0), 0.0), Layout::top_down(Align::Min), |ui| {
                        self.row_text(ui, title, description);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), control).inner
                })
                .inner
            }
        });
        ui.separator();
        r
    }

    fn row_text(&self, ui: &mut Ui, title: &str, description: &str) {
        ui.add(Label::new(RichText::new(title).font(super::font_weight(text::BODY.size, 500)).color(colour(self.palette.text))).wrap());
        if !description.is_empty() {
            ui.add(Label::new(RichText::new(description).font(super::font(text::SMALL)).color(colour(self.palette.muted))).wrap());
        }
    }

    /// The whole row toggles, including its label. Keyboard Space/Enter work
    /// through egui's normal click sense; disabled rows cannot change state.
    pub fn switch_row(&self, ui: &mut Ui, value: &mut bool, title: &str, description: &str) -> Response {
        let width = ui.available_width();
        let row = ui.horizontal(|ui| {
            let label = ui
                .allocate_ui_with_layout(Vec2::new((width - 56.0).max(0.0), size::ROW), Layout::top_down(Align::Min), |ui| {
                    self.row_text(ui, title, description);
                })
                .response;
            (label, self.switch(ui, value, title))
        });
        let (label_area, mut r) = row.inner;
        let label_rect = Rect::from_min_max(row.response.rect.min, ::egui::pos2(r.rect.left(), row.response.rect.bottom()));
        let label = ui.interact(label_rect.union(label_area.rect), r.id.with("label"), Sense::CLICK);
        if label.clicked() && ui.is_enabled() {
            *value = !*value;
            r.mark_changed();
        }
        ui.separator();
        r | label
    }

    pub fn switch(&self, ui: &mut Ui, value: &mut bool, label: &str) -> Response {
        let (rect, mut r) = ui.allocate_exact_size(Vec2::new(40.0, size::CONTROL_SM), Sense::click());
        if r.clicked() && ui.is_enabled() {
            *value = !*value;
            r.mark_changed();
        }
        r.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *value, label));
        let track = Rect::from_center_size(rect.center(), Vec2::new(32.0, 18.0));
        let fill = if !ui.is_enabled() {
            self.palette.line_strong
        } else if *value {
            self.palette.accent
        } else {
            self.palette.muted
        };
        ui.painter().rect_filled(track, radius::PILL, colour(fill));
        let center = ::egui::pos2(if *value { track.right() - 9.0 } else { track.left() + 9.0 }, track.center().y);
        let knob = if !ui.is_enabled() {
            self.palette.muted
        } else if *value {
            self.palette.accent_text
        } else {
            self.palette.knob
        };
        ui.painter().circle_filled(center, 6.0, colour(knob));
        self.focus(ui, &r);
        r
    }

    pub fn segmented<T: Copy + PartialEq>(&self, ui: &mut Ui, value: &mut T, choices: &[(T, &str)]) -> Response {
        let mut combined = ui.allocate_response(Vec2::ZERO, Sense::hover());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for &(option, label) in choices {
                let active = *value == option;
                let mut r = self.button(ui, label, if active { ButtonKind::Primary } else { ButtonKind::Default });
                if r.clicked() && !active {
                    *value = option;
                    r.mark_changed();
                }
                r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), active, label));
                combined |= r;
            }
        });
        combined
    }

    pub fn text_input(&self, ui: &mut Ui, value: &mut String, hint: &str, invalid: Option<&str>) -> Response {
        let r = ui.add_sized(
            [ui.available_width(), size::CONTROL],
            TextEdit::singleline(value).font(super::font(text::CONTROL)).hint_text(RichText::new(hint).color(colour(self.palette.muted))),
        );
        self.focus(ui, &r);
        if let Some(message) = invalid {
            ui.painter().rect_stroke(r.rect, radius::SM, Stroke::new(1.0, colour(self.ink.danger)), StrokeKind::Inside);
            ui.add(Label::new(RichText::new(message).font(super::font(text::SMALL)).color(colour(self.ink.danger))).wrap());
        }
        r
    }

    pub fn select<T: PartialEq + Clone>(&self, ui: &mut Ui, id: impl std::hash::Hash, value: &mut T, options: &[(T, &str)]) -> Response {
        let selected = options.iter().find(|(v, _)| v == value).map_or("Choose…", |(_, label)| *label);
        let before = value.clone();
        let mut r = ::egui::ComboBox::from_id_salt(id)
            .selected_text(selected)
            .width(ui.available_width().min(240.0))
            .truncate()
            .show_ui(ui, |ui| {
                for (option, label) in options {
                    ui.selectable_value(value, option.clone(), *label);
                }
            })
            .response;
        if before != *value {
            r.mark_changed();
        }
        self.focus(ui, &r);
        r
    }

    pub fn badge(&self, ui: &mut Ui, label: &str, tone: Tone) -> Response {
        let (fg, bg) = self.tones(tone);
        Frame::new()
            .fill(bg)
            .corner_radius(radius::XS)
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| ui.add(Label::new(RichText::new(label).font(super::font(text::CAPTION)).color(fg)).truncate()).on_hover_text(label))
            .inner
    }

    pub fn key_chip(&self, ui: &mut Ui, key: &str) -> Response {
        Frame::new()
            .fill(colour(self.palette.sunken))
            .stroke(Stroke::new(1.0, colour(self.palette.line_strong)))
            .corner_radius(radius::XS)
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| ui.add(Label::new(RichText::new(key).font(super::font(text::CAPTION)).color(colour(self.palette.muted)))))
            .inner
    }

    pub fn notice(&self, ui: &mut Ui, message: &str, tone: Tone) -> Response {
        self.notice_action(ui, message, tone, |_| ()).response
    }

    pub fn notice_action<R>(&self, ui: &mut Ui, message: &str, tone: Tone, action: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let (fg, bg) = self.tones(tone);
        Frame::new().fill(bg).corner_radius(radius::MD).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(Label::new(RichText::new(message).font(super::font(text::SMALL)).color(fg)).wrap());
            action(ui)
        })
    }

    pub fn empty_state<R>(&self, ui: &mut Ui, icon: &str, title: &str, description: &str, actions: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        Frame::new().stroke(Stroke::new(1.0, colour(self.palette.line_strong))).corner_radius(radius::MD).inner_margin(24.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(size::TILE), Sense::hover());
                self.paint_icon(ui, icon, r.center(), colour(self.palette.muted));
                self.row_text(ui, title, description);
                actions(ui)
            })
            .inner
        })
    }

    pub fn skeleton_row(&self, ui: &mut Ui) {
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), size::ROW), Sense::hover());
        let bar = Rect::from_min_size(r.min + Vec2::new(0.0, 8.0), Vec2::new(r.width() * 0.6, 12.0));
        ui.painter().rect_filled(bar, radius::XS, colour(self.palette.sunken));
        ui.painter().rect_filled(bar.translate(Vec2::new(0.0, 20.0)).shrink2(Vec2::new(bar.width() * 0.15, 2.0)), radius::XS, colour(self.palette.sunken));
    }

    pub fn path(&self, ui: &mut Ui, path: &str) -> Response {
        ui.add(Label::new(RichText::new(path).monospace().size(text::MONO.size).color(colour(self.palette.muted))).wrap().selectable(true))
    }

    pub fn ellipsis(&self, ui: &mut Ui, name: &str) -> Response {
        ui.add(Label::new(RichText::new(name).color(colour(self.palette.text))).truncate()).on_hover_text(name)
    }

    fn focus(&self, ui: &Ui, r: &Response) {
        if r.has_focus() {
            ui.painter().rect_stroke(
                r.rect.expand(size::FOCUS_OFFSET),
                radius::SM,
                Stroke::new(size::FOCUS_RING, colour(self.palette.accent)),
                StrokeKind::Outside,
            );
        }
        if r.hovered() && ui.is_enabled() {
            ui.ctx().set_cursor_icon(::egui::CursorIcon::PointingHand);
        }
    }

    /// Draw an outline from the shared icon table, including cubic segments.
    pub fn paint_icon(&self, ui: &Ui, name: &str, center: ::egui::Pos2, color: Color32) {
        let Some(icon) = crate::design::icon(name) else { return };
        let scale = size::ICON / 24.0;
        let origin = center - Vec2::splat(size::ICON * 0.5);
        let point = |x, y| origin + Vec2::new(x, y);
        let mut points = Vec::new();
        let mut current = origin;
        let flush = |points: &mut Vec<::egui::Pos2>, closed: bool| {
            if points.len() > 1 {
                ui.painter().add(::egui::epaint::PathShape {
                    points: std::mem::take(points),
                    closed,
                    fill: if icon.filled { color } else { Color32::TRANSPARENT },
                    stroke: Stroke::new(size::ICON_STROKE, color).into(),
                });
            }
        };
        for op in icon.path(scale) {
            match op {
                PathOp::Move(x, y) => {
                    flush(&mut points, false);
                    current = point(x, y);
                    points.push(current);
                }
                PathOp::Line(x, y) => {
                    current = point(x, y);
                    points.push(current);
                }
                PathOp::Cubic(a, b, c, d, x, y) => {
                    let (p0, p1, p2, p3) = (current, point(a, b), point(c, d), point(x, y));
                    for step in 1..=12 {
                        let t = step as f32 / 12.0;
                        let s = 1.0 - t;
                        points.push(
                            (p0.to_vec2() * s.powi(3) + p1.to_vec2() * (3.0 * s * s * t) + p2.to_vec2() * (3.0 * s * t * t) + p3.to_vec2() * t.powi(3))
                                .to_pos2(),
                        );
                    }
                    current = p3;
                }
                PathOp::Close => flush(&mut points, true),
            }
        }
        flush(&mut points, false);
    }

    /// Template A navigation. Arrows move between items; Enter/Space also use
    /// egui's normal button activation. Collapsed mode contains icons/version.
    pub fn sidebar(&self, ctx: &Context, app_name: &str, version: &str, items: &[NavItem<'_>], selected: &mut usize) -> bool {
        self.sidebar_with_brand(ctx, app_name, version, None, items, selected)
    }

    /// Supply the app's kit glyph as a white mask texture for its neutral
    /// brand tile. It is tinted to the current background in both themes.
    pub fn sidebar_with_brand(
        &self,
        ctx: &Context,
        app_name: &str,
        version: &str,
        brand: Option<::egui::TextureId>,
        items: &[NavItem<'_>],
        selected: &mut usize,
    ) -> bool {
        let old = *selected;
        let collapsed = ctx.content_rect().width() < size::COLLAPSE_BELOW;
        let width = if collapsed { size::SIDEBAR_COLLAPSED } else { size::SIDEBAR };
        ::egui::SidePanel::left("arcade-sidebar").exact_width(width).resizable(false).show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let (tile, _) = ui.allocate_exact_size(Vec2::splat(size::TILE_SM), Sense::hover());
                ui.painter().rect_filled(tile, radius::TILE_SM, colour(self.palette.text));
                if let Some(texture) = brand {
                    ui.painter().image(texture, tile.shrink(6.0), Rect::from_min_max(::egui::pos2(0.0, 0.0), ::egui::pos2(1.0, 1.0)), colour(self.palette.bg));
                } else {
                    self.paint_icon(ui, "apps", tile.center(), colour(self.palette.bg));
                }
                if !collapsed {
                    ui.label(RichText::new(app_name).font(super::font(text::HEADING)).strong());
                }
            });
            ui.add_space(20.0);
            for (i, item) in items.iter().enumerate() {
                let active = i == *selected;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), size::NAV), Sense::hover());
                let id = Id::new("arcade-nav").with(i);
                let mut r = ui.interact(rect, id, if active { Sense::click() } else { Sense::CLICK });
                if r.clicked() {
                    *selected = i;
                }
                if r.has_focus() {
                    let dir = ui.input(|input| {
                        if input.key_pressed(Key::ArrowDown) {
                            1
                        } else if input.key_pressed(Key::ArrowUp) {
                            -1
                        } else {
                            0
                        }
                    });
                    if dir != 0 && !items.is_empty() {
                        *selected = (i as isize + dir).rem_euclid(items.len() as isize) as usize;
                        ui.memory_mut(|m| m.request_focus(Id::new("arcade-nav").with(*selected)));
                    }
                }
                if active || r.hovered() {
                    ui.painter().rect(
                        rect,
                        radius::SM,
                        colour(if active { self.palette.panel } else { self.palette.hover }),
                        Stroke::new(1.0, colour(self.palette.line)),
                        StrokeKind::Inside,
                    );
                }
                self.paint_icon(ui, item.icon, rect.left_center() + Vec2::new(18.0, 0.0), colour(if active { self.ink.accent } else { self.palette.muted }));
                if !collapsed {
                    ui.painter().text(
                        rect.left_center() + Vec2::new(36.0, 0.0),
                        Align2::LEFT_CENTER,
                        item.label,
                        super::font_weight(text::BODY.size, 500),
                        colour(self.palette.text),
                    );
                    if let Some(count) = item.count {
                        ui.painter().text(
                            rect.right_center() - Vec2::new(8.0, 0.0),
                            Align2::RIGHT_CENTER,
                            crate::design::format::format_count(count.min(i64::MAX as usize) as i64),
                            super::font(text::CAPTION),
                            colour(self.palette.muted),
                        );
                    }
                }
                r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), active, item.label));
                self.focus(ui, &r);
                r = r.on_hover_text(item.label);
                if i == *selected && old != *selected {
                    r.request_focus();
                }
            }
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                let label = if collapsed { version.to_string() } else { format!("Version {version}") };
                ui.add(Label::new(RichText::new(label).font(super::font(text::CAPTION)).color(colour(self.palette.muted))).selectable(true));
            });
        });
        old != *selected
    }
}

/// Local toast state; errors stay until explicitly closed. Other messages
/// expire after five seconds, and copy feedback after 1.5 seconds.
pub struct Toast {
    pub message: String,
    pub tone: Tone,
    created: Instant,
    lifetime: Option<Duration>,
}

impl Toast {
    pub fn new(message: impl Into<String>, tone: Tone) -> Self {
        Self { message: message.into(), tone, created: Instant::now(), lifetime: (tone != Tone::Danger).then(|| Duration::from_millis(motion::TOAST_MS)) }
    }

    pub fn copied() -> Self {
        let mut toast = Self::new("Copied", Tone::Ok);
        toast.lifetime = Some(Duration::from_millis(motion::COPIED_MS));
        toast
    }

    /// Returns false when dismissed or expired, so the app can drop it.
    pub fn show(&self, ctx: &Context, kit: &Kit) -> bool {
        if self.lifetime.is_some_and(|lifetime| self.created.elapsed() >= lifetime) {
            return false;
        }
        let mut open = true;
        ::egui::Area::new(Id::new("arcade-toast")).anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0]).order(::egui::Order::Foreground).show(ctx, |ui| {
            ui.set_max_width(360.0_f32.min(ctx.content_rect().width() - 32.0));
            kit.notice_action(ui, &self.message, self.tone, |ui| {
                if kit.icon_button(ui, "x", "Close notification").clicked() {
                    open = false;
                }
            });
        });
        if let Some(lifetime) = self.lifetime {
            ctx.request_repaint_after(lifetime.saturating_sub(self.created.elapsed()));
        }
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::egui::{Event, Modifiers, PointerButton, RawInput};

    fn input(width: f32, height: f32, events: Vec<Event>) -> RawInput {
        RawInput { screen_rect: Some(Rect::from_min_size(::egui::Pos2::ZERO, Vec2::new(width, height))), events, ..RawInput::default() }
    }

    fn click(position: ::egui::Pos2) -> Vec<Event> {
        vec![
            Event::PointerMoved(position),
            Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
            Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        ]
    }

    #[test]
    fn switch_label_and_keyboard_toggle_and_disabled_state_does_not() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let mut value = false;
        let draw = |events, enabled, value: &mut bool| {
            let mut row = Rect::NOTHING;
            let mut changed = false;
            let mut id = Id::NULL;
            let _ = ctx.run(input(720.0, 520.0, events), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let r = kit.switch_row(ui, value, "Start at login", "Run in the background");
                        row = ui.min_rect();
                        changed = r.changed();
                        id = r.id;
                    });
                });
            });
            (row, changed, id)
        };
        let (row, _, _) = draw(vec![], true, &mut value);
        let position = row.left_top() + Vec2::new(10.0, 10.0);
        let (_, changed, id) = draw(click(position), true, &mut value);
        assert!(changed && value, "clicking the label must toggle the switch");
        ctx.memory_mut(|m| m.request_focus(id));
        let key = Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let (_, changed, _) = draw(vec![key], true, &mut value);
        assert!(changed && !value, "Space must toggle the focused switch");
        let (_, changed, _) = draw(click(position), false, &mut value);
        assert!(!changed && !value, "a disabled label must not toggle");
    }

    #[test]
    fn component_gallery_fits_minimum_window_in_both_themes_and_text_scales() {
        for theme in [Theme::Light, Theme::Dark] {
            for scale in [1.0, 1.25] {
                let ctx = Context::default();
                super::super::install_inter(&ctx);
                super::super::apply(&ctx, if theme.is_dark() { "dark" } else { "light" }, "arcade.find");
                ctx.set_zoom_factor(scale);
                let kit = Kit::for_theme(theme, "arcade.find");
                let mut selected = 0;
                let mut enabled = true;
                let mut appearance = 0;
                let mut query = String::new();
                let mut option = 0;
                // Font installation and egui auto-sizing settle over two frames.
                for _ in 0..2 {
                    let output = ctx.run(input(720.0, 520.0, vec![]), |ctx| {
                        kit.sidebar(ctx, "Arcade Find", "0.5.0", &[NavItem { label: "General", icon: "settings", count: None }], &mut selected);
                        ::egui::CentralPanel::default().show(ctx, |ui| {
                            let available = ui.available_rect_before_wrap();
                            ::egui::ScrollArea::vertical().show(ui, |ui| {
                                kit.page_head(ui, "General", "Choose how Arcade Find looks and opens.");
                                kit.section_head(ui, "Appearance");
                                kit.list(ui, |ui| {
                                    kit.row(ui, "Theme", "Follow the system or choose a theme", |ui| {
                                        kit.segmented(ui, &mut appearance, &[(0, "System"), (1, "Light"), (2, "Dark")]);
                                    });
                                    kit.switch_row(
                                        ui,
                                        &mut enabled,
                                        "Start at login",
                                        "A long setting description that must wrap and stay readable at the minimum width and at increased text size.",
                                    );
                                    kit.text_input(ui, &mut query, "Search files", Some("Couldn't read the folder. Choose another folder."));
                                    kit.select(ui, "locations", &mut option, &[(0, "Home"), (1, "Documents")]);
                                    kit.path(ui, "/a/very/long/folder/with/many/segments/and/a/long-filename-that-needs-to-wrap.txt");
                                    kit.ellipsis(ui, "A very long result name that needs an ellipsis and a tooltip instead of stretching the panel");
                                });
                                ui.horizontal_wrapped(|ui| {
                                    for tone in [Tone::Neutral, Tone::Ok, Tone::Warn, Tone::Danger, Tone::Accent] {
                                        kit.badge(ui, "Connected", tone);
                                    }
                                    kit.key_chip(ui, "Ctrl");
                                    kit.icon_button(ui, "settings", "Settings");
                                });
                                kit.notice(ui, "Couldn't open the folder. Check its permissions and retry.", Tone::Danger);
                                kit.empty_state(ui, "search", "No files yet", "Choose a folder to begin indexing.", |ui| {
                                    kit.button(ui, "Add folder", ButtonKind::Primary);
                                });
                                kit.skeleton_row(ui);
                                assert!(ui.min_rect().right() <= available.right() + 1.0, "{theme:?} scale {scale}: horizontal overflow");
                            });
                        });
                    });
                    assert!(!output.shapes.is_empty(), "the gallery must render real egui shapes");
                }
            }
        }
    }

    #[test]
    fn toast_expiration_and_copy_feedback_follow_the_standard() {
        let mut toast = Toast::new("Updated", Tone::Ok);
        toast.created -= Duration::from_secs(6);
        assert!(!toast.show(&Context::default(), &Kit::for_theme(Theme::Light, "arcade.find")));
        let error = Toast::new("Couldn't save settings", Tone::Danger);
        assert!(error.lifetime.is_none(), "errors stay until dismissed");
        let copy = Toast::copied();
        assert_eq!(copy.message, "Copied");
        assert_eq!(copy.lifetime, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn recorder_requires_modifiers_except_function_keys_and_normalizes_arrows() {
        assert!(recorded_accelerator(Key::F, Modifiers::NONE).is_err());
        assert_eq!(recorded_accelerator(Key::F8, Modifiers::NONE).unwrap(), "F8");
        assert_eq!(recorded_accelerator(Key::F, Modifiers { ctrl: true, alt: true, ..Modifiers::NONE }).unwrap(), "Ctrl+Alt+F");
        assert_eq!(recorded_accelerator(Key::ArrowDown, Modifiers { shift: true, ..Modifiers::NONE }).unwrap(), "Shift+Down");
    }

    #[test]
    fn busy_buttons_keep_their_width_and_cannot_be_activated() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let draw = |busy, events| {
            let mut result = (Rect::NOTHING, false);
            let _ = ctx.run(input(720.0, 520.0, events), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    let r = kit.button_state(ui, "Rescan now", ButtonKind::Primary, true, busy, false);
                    result = (r.rect, r.clicked());
                });
            });
            result
        };
        let (idle, _) = draw(false, vec![]);
        let (busy, clicked) = draw(true, click(idle.center()));
        assert_eq!(idle.size(), busy.size());
        assert!(!clicked);
        draw(false, vec![]);
        let (_, clicked) = draw(false, click(idle.center()));
        assert!(clicked, "the idle button must remain actionable");
    }
}
