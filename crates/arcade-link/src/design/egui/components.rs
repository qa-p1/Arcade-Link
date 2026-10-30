//! Shared egui controls for Arcade windows and overlays (STANDARD §5–7).
//!
//! Call [`super::install_inter`] once, then [`super::apply`] when Appearance
//! changes. Construct a [`Kit`] inside each frame so System follows egui's live
//! native theme. Apps supply data and actions; the kit owns layout and styling.

use super::colour;
use crate::design::{ink, motion, palette, radius, size, text, Ink, Palette, PathOp, Theme};
use ::egui::{
    Align, Align2, Color32, Context, Frame, Id, InnerResponse, Key, Label, Layout, Margin, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextEdit, Ui,
    UiBuilder, Vec2, WidgetInfo, WidgetType,
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

/// Horizontal padding of list rows and notices (`.row`, `.notice`).
const ROW_PAD: f32 = 14.0;

/// Measured sizes for [`Kit::beside`], kept between passes.
#[derive(Clone, Copy, Debug, Default)]
struct Beside {
    leading: f32,
    trailing: Vec2,
}

impl Beside {
    fn close_to(self, other: Self) -> bool {
        (self.leading - other.leading).abs() < 0.5 && (self.trailing - other.trailing).length() < 0.5
    }
}

fn tone_icon(tone: Tone) -> &'static str {
    match tone {
        Tone::Ok => "check",
        Tone::Warn | Tone::Danger => "alert",
        Tone::Neutral | Tone::Accent => "info",
    }
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

/// What a shortcut row shows: its label, description and default binding.
pub struct ShortcutSpec<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub default: &'a str,
    /// Backspace while recording clears the binding.
    pub allow_clear: bool,
}

impl ShortcutRecorder {
    /// A settings row: the binding's key chips, "Change…" and (when it differs
    /// from the default) "Reset" beside the label, all 28 px high; conflicts and
    /// errors appear under the row. Returns true when `value` changed.
    pub fn row(&mut self, ui: &mut Ui, kit: &Kit, spec: &ShortcutSpec<'_>, value: &mut String, conflict: impl Fn(&str) -> Option<String>) -> bool {
        let candidate = kit.row(ui, spec.title, spec.description, |ui| self.controls(ui, kit, value, spec.default, spec.allow_clear)).inner;
        let mut changed = self.resolve(candidate, value, conflict);
        if self.pending.is_some() || self.error.is_some() {
            changed |= self.feedback(ui, kit, value);
            ui.add_space(10.0);
        }
        changed
    }

    /// The recorder on its own, with any feedback below it.
    pub fn show(&mut self, ui: &mut Ui, kit: &Kit, value: &mut String, default: &str, allow_clear: bool, conflict: impl Fn(&str) -> Option<String>) -> bool {
        let candidate = ui.horizontal(|ui| self.controls(ui, kit, value, default, allow_clear)).inner;
        let changed = self.resolve(candidate, value, conflict);
        changed | self.feedback(ui, kit, value)
    }

    fn controls(&mut self, ui: &mut Ui, kit: &Kit, value: &str, default: &str, allow_clear: bool) -> Option<String> {
        let mut candidate = None;
        ui.spacing_mut().item_spacing.x = 6.0;
        if self.recording {
            let r = kit.button_state(ui, "Press keys…", ButtonKind::Primary, true, false, true);
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
            if kit.button_state(ui, "Cancel", ButtonKind::Ghost, true, false, true).clicked() {
                self.recording = false;
            }
        } else {
            let platform = if cfg!(target_os = "macos") {
                crate::accelerator::Platform::Macos
            } else if cfg!(windows) {
                crate::accelerator::Platform::Windows
            } else {
                crate::accelerator::Platform::Linux
            };
            if value.is_empty() {
                ui.label(RichText::new("None").font(super::font(text::SMALL)).color(colour(kit.palette.muted)));
            } else {
                let display = crate::accelerator::display(value, platform).unwrap_or_else(|_| value.to_string());
                ui.spacing_mut().item_spacing.x = 4.0;
                for key in display.split('+') {
                    kit.key_chip(ui, key);
                }
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 6.0;
            }
            if kit.button_state(ui, "Change…", ButtonKind::Default, true, false, true).clicked() {
                self.recording = true;
                self.pending = None;
                self.error = None;
            }
            if value != default && kit.button_state(ui, "Reset", ButtonKind::Ghost, true, false, true).on_hover_text("Reset to default").clicked() {
                candidate = Some(default.to_string());
            }
        }
        candidate
    }

    fn resolve(&mut self, candidate: Option<String>, value: &mut String, conflict: impl Fn(&str) -> Option<String>) -> bool {
        let Some(candidate) = candidate else { return false };
        self.error = None;
        self.recording = false;
        if let Some(owner) = (!candidate.is_empty()).then(|| conflict(&candidate)).flatten() {
            self.pending = Some((candidate, owner));
            false
        } else {
            self.pending = None;
            let changed = *value != candidate;
            *value = candidate;
            changed
        }
    }

    fn feedback(&mut self, ui: &mut Ui, kit: &Kit, value: &mut String) -> bool {
        let mut changed = false;
        if let Some((candidate, owner)) = self.pending.clone() {
            kit.notice_action(ui, &format!("Used by {owner}"), Tone::Warn, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if kit.button_state(ui, "Use anyway", ButtonKind::Default, true, false, true).clicked() {
                    changed = *value != candidate;
                    *value = candidate;
                    self.pending = None;
                }
                if kit.button_state(ui, "Choose another", ButtonKind::Ghost, true, false, true).clicked() {
                    self.pending = None;
                    self.recording = true;
                }
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
        // Matches components.css `.btn`: line-strong borders, a borderless muted
        // ghost, and danger's red border only on hover.
        let p = self.palette;
        let enabled = enabled && ui.is_enabled();
        let font = super::font(if small { text::SMALL } else { text::CONTROL });
        let natural_width = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label.to_string(), font.clone(), Color32::PLACEHOLDER)).size().x;
        // `.btn` padding (12 px, 10 px small). Busy swaps the label for a
        // centred spinner, so the width never changes. Wrapping rows move a
        // button to the next line, so only the whole row width bounds it.
        let pad = if small { 10.0 } else { 12.0 };
        let width = (natural_width + 2.0 * pad).max(size::HIT_TARGET).min(ui.max_rect().width());
        let height = if small { size::CONTROL_SM } else { size::CONTROL };
        // Allocated directly in the caller's layout (not a child scope) so
        // wrapping rows can move the button to the next line.
        let sense = if enabled && !busy { Sense::click() } else { Sense::hover() };
        let r = ui.allocate_exact_size(Vec2::new(width, height), sense).1;
        let hovered = r.hovered() && enabled && !busy;
        let (fill, fg, border) = match (kind, hovered) {
            (ButtonKind::Default, false) => (Some(p.panel), p.text, Some(p.line_strong)),
            (ButtonKind::Default, true) => (Some(p.hover), p.text, Some(p.line_strong)),
            (ButtonKind::Primary, _) => (Some(p.accent), p.accent_text, Some(p.accent)),
            (ButtonKind::Ghost, false) => (None, p.muted, None),
            (ButtonKind::Ghost, true) => (Some(p.hover), p.text, None),
            (ButtonKind::Danger, false) => (Some(p.panel), self.ink.danger, Some(p.line_strong)),
            (ButtonKind::Danger, true) => (Some(p.danger_soft), self.ink.danger, Some(p.danger)),
            (ButtonKind::Destructive, _) => (Some(p.danger), p.on_fill, Some(p.danger)),
        };
        let (fill, fg, border) = if enabled { (fill, fg, border) } else { (fill.map(|_| p.sunken), p.muted, border.map(|_| p.line_strong)) };
        let pressed = r.is_pointer_button_down_on() && enabled && !busy;
        ui.painter().rect(
            r.rect,
            radius::SM,
            fill.map_or(Color32::TRANSPARENT, colour),
            border.map_or(Stroke::NONE, |border| Stroke::new(if pressed { 2.0 } else { 1.0 }, colour(border))),
            StrokeKind::Inside,
        );
        if busy {
            let rect = Rect::from_center_size(r.rect.center(), Vec2::splat(size::ICON));
            ::egui::Spinner::new().color(colour(fg)).paint_at(ui, rect);
        } else {
            let mut job = ::egui::text::LayoutJob::simple_singleline(label.to_string(), font, colour(fg));
            job.wrap.max_width = (width - 2.0 * pad).max(0.0);
            job.wrap.max_rows = 1;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            ui.painter().galley(r.rect.center() - galley.size() * 0.5, galley, colour(fg));
        }
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled && !busy, label));
        self.focus(ui, &r);
        // Only a shortened label needs its full text as a tooltip.
        if natural_width > width - 2.0 * pad {
            r.on_hover_text(label)
        } else {
            r
        }
    }

    /// A ghost icon-only button (`.btn.ghost.icon-only.sm`); `label` is its
    /// accessible name and tooltip.
    pub fn icon_button(&self, ui: &mut Ui, name: &str, label: &str) -> Response {
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(size::CONTROL_SM), Sense::click());
        let enabled = ui.is_enabled();
        let hovered = r.hovered() && enabled;
        if hovered {
            ui.painter().rect_filled(rect, radius::SM, colour(self.palette.hover));
        }
        let fg = if !enabled {
            self.palette.line_strong
        } else if hovered {
            self.palette.text
        } else {
            self.palette.muted
        };
        self.paint_icon(ui, name, rect.center(), colour(fg));
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
        self.focus(ui, &r);
        r.on_hover_text(label)
    }

    /// Template A content: the page background, 32/36 px padding, a vertical
    /// scroll owned by the page, and the kit's maximum content width.
    pub fn page<R>(&self, ctx: &Context, contents: impl FnOnce(&mut Ui) -> R) -> R {
        ::egui::CentralPanel::default()
            .frame(Frame::new().fill(colour(self.palette.bg)))
            .show(ctx, |ui| {
                ::egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // `.page-main`: min(100%, content-max) wide, centred, with
                        // 36 px sides inside that width.
                        let full = ui.available_width();
                        let width = (full.min(size::CONTENT_MAX) - 72.0).max(0.0);
                        ui.add_space(32.0);
                        let inner = ui
                            .horizontal_top(|ui| {
                                ui.add_space(((full - width) / 2.0 - ui.spacing().item_spacing.x).max(0.0));
                                ui.vertical(|ui| {
                                    ui.set_width(width);
                                    contents(ui)
                                })
                                .inner
                            })
                            .inner;
                        ui.add_space(72.0);
                        inner
                    })
                    .inner
            })
            .inner
    }

    pub fn page_head(&self, ui: &mut Ui, title: &str, description: &str) {
        self.page_head_with(ui, title, description, |_| ());
    }

    /// Template A with a fixed page head and a separately scrolling body.
    /// Long lists scroll below the title without moving the sidebar or head.
    pub fn page_with_head<R>(&self, ctx: &Context, title: &str, description: &str, contents: impl FnOnce(&mut Ui) -> R) -> R {
        self.page_with_head_actions(ctx, title, description, |_| (), contents)
    }

    /// [`Kit::page_with_head`] with actions at the trailing edge of the head.
    pub fn page_with_head_actions<R>(
        &self,
        ctx: &Context,
        title: &str,
        description: &str,
        actions: impl FnOnce(&mut Ui),
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        ::egui::CentralPanel::default()
            .frame(Frame::new().fill(colour(self.palette.bg)))
            .show(ctx, |ui| {
                let full = ui.available_width();
                let width = (full.min(size::CONTENT_MAX) - 72.0).max(0.0);
                ui.add_space(32.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(((full - width) / 2.0 - ui.spacing().item_spacing.x).max(0.0));
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        self.page_head_with(ui, title, description, actions);
                        ::egui::ScrollArea::vertical()
                            .id_salt(("arcade-page-body", title))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let result = contents(ui);
                                ui.add_space(72.0);
                                result
                            })
                            .inner
                    })
                    .inner
                })
                .inner
            })
            .inner
    }

    /// `.page-head` with `.page-actions` at the trailing edge.
    pub fn page_head_with<R>(&self, ui: &mut Ui, title: &str, description: &str, actions: impl FnOnce(&mut Ui) -> R) -> R {
        let id = ui.id().with(("arcade-page-head", title));
        let inner = self.beside(
            ui,
            id,
            0.0,
            |ui| {
                ui.label(RichText::new(title).font(super::font(text::TITLE)).color(colour(self.palette.text)));
                ui.add_space(2.0);
                ui.add(Label::new(RichText::new(description).font(super::font(text::SMALL)).color(colour(self.palette.muted))).wrap());
            },
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                actions(ui)
            },
        );
        ui.add_space(24.0);
        let end = ui.cursor().top();
        ui.data_mut(|d| d.insert_temp(ui.id().with("arcade-page-head-end"), end));
        inner
    }

    pub fn section_head(&self, ui: &mut Ui, title: &str) {
        self.section_head_with(ui, title, |_| ());
    }

    /// `.section`: 32 px above every section except the first on the page, and
    /// an optional trailing action (for example "Add folder").
    pub fn section_head_with<R>(&self, ui: &mut Ui, title: &str, actions: impl FnOnce(&mut Ui) -> R) -> R {
        let after_head = ui.data(|d| d.get_temp::<f32>(ui.id().with("arcade-page-head-end"))).is_some_and(|end| (end - ui.cursor().top()).abs() < 1.0);
        if !after_head && ui.cursor().top() > ui.max_rect().top() + 0.5 {
            ui.add_space(32.0);
        }
        let id = ui.id().with(("arcade-section-head", title));
        let inner = self.beside(
            ui,
            id,
            0.0,
            |ui| {
                ui.label(RichText::new(title).font(super::font(text::HEADING)).color(colour(self.palette.text)));
            },
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                actions(ui)
            },
        );
        ui.add_space(10.0);
        inner
    }

    /// A bounded list panel (`.list`); scrolling is owned by the app's page.
    /// Put [`Kit::row`]s and [`Kit::switch_row`]s inside it.
    pub fn list<R>(&self, ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        Frame::new()
            .fill(colour(self.palette.panel))
            .stroke(Stroke::new(1.0, colour(self.palette.line)))
            .corner_radius(radius::MD)
            .inner_margin(Margin::symmetric(ROW_PAD as i8, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                contents(ui)
            })
    }

    /// Label and optional description, with an app-supplied trailing control
    /// (`.setting-row`). The control keeps its natural width and the text gets
    /// the rest; a control wider than about half the row moves below the text.
    pub fn row<R>(&self, ui: &mut Ui, title: &str, description: &str, control: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.row_divider(ui);
        // A label is not an identity: shortcut sheets and data lists repeat names.
        let id = ui.next_auto_id().with(("arcade-row", title));
        ui.vertical(|ui| {
            // Exactly 10 px above and below: no item spacing inside the row.
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(10.0);
            let inner = self.beside(ui, id, size::ROW - 20.0, |ui| self.row_text(ui, title, description), control);
            ui.add_space(10.0);
            inner
        })
    }

    /// `.row + .row`: a line between rows, none above the first or below the last.
    fn row_divider(&self, ui: &mut Ui) {
        if ui.cursor().top() > ui.max_rect().top() + 0.5 {
            let x = ui.max_rect().x_range().expand(ROW_PAD);
            ui.painter().hline(x, ui.cursor().top(), Stroke::new(1.0, colour(self.palette.line)));
        }
    }

    /// Leading content beside a trailing part that keeps its natural size,
    /// both centred on one line at least `min_height` high. egui lays out in a
    /// single pass, so both sides are measured and the pass discarded when a
    /// size changes: text never runs under the trailing part, multi-part
    /// controls keep their left-to-right order, and a trailing part that
    /// leaves the text less than 40 % of the width moves below it.
    fn beside<R>(&self, ui: &mut Ui, id: Id, min_height: f32, leading: impl FnOnce(&mut Ui), trailing: impl FnOnce(&mut Ui) -> R) -> R {
        const GAP: f32 = 12.0;
        let width = ui.available_width();
        let known = ui.data(|d| d.get_temp::<Beside>(id));
        let min_lead = (width * 0.4).max(150.0).min(width);
        let fits = known.filter(|m| m.trailing.x + GAP <= width - min_lead);
        let (inner, now) = if let Some(m) = fits {
            let gap = if m.trailing.x > 0.0 { GAP } else { 0.0 };
            let lead_width = (width - m.trailing.x - gap).max(0.0);
            let height = m.leading.max(m.trailing.y).max(min_height);
            let rect = Rect::from_min_size(ui.next_widget_position(), Vec2::new(width, height));
            let lead_rect = Rect::from_min_size(::egui::pos2(rect.left(), rect.center().y - m.leading * 0.5), Vec2::new(lead_width, m.leading));
            let mut lead = ui.new_child(UiBuilder::new().id_salt(id.with("leading")).max_rect(lead_rect).layout(Layout::top_down(Align::Min)));
            lead.set_width(lead_width);
            leading(&mut lead);
            // A changed description or width may need more height than the
            // previous measurement. Reserve its actual bounds in this pass;
            // a discarded pass can still be painted when the pass budget ends.
            let height = (lead.min_rect().bottom() - rect.top()).max(m.trailing.y).max(min_height);
            let trail_rect = Rect::from_min_size(::egui::pos2(rect.right() - m.trailing.x, rect.top() + (height - m.trailing.y) * 0.5), m.trailing);
            let mut trail = ui.new_child(UiBuilder::new().id_salt(id.with("trailing")).max_rect(trail_rect).layout(Layout::left_to_right(Align::Center)));
            let inner = trailing(&mut trail);
            let now = Beside { leading: lead.min_rect().height(), trailing: trail.min_rect().size() };
            let bottom = lead.min_rect().bottom().max(trail.min_rect().bottom()).max(rect.top() + min_height);
            ui.advance_cursor_after_rect(Rect::from_min_max(rect.min, ::egui::pos2(rect.right(), bottom)));
            (inner, now)
        } else {
            // Unmeasured (the first pass) or too wide: stacked, which can't overflow.
            let lead = ui.scope(leading).response;
            if known.is_some() {
                ui.add_space(6.0);
            }
            let r = ui.horizontal_wrapped(trailing);
            let size = r.response.rect.size();
            (r.inner, Beside { leading: lead.rect.height(), trailing: if size.x > 0.5 { size } else { Vec2::ZERO } })
        };
        if !known.is_some_and(|m| m.close_to(now)) {
            ui.data_mut(|d| d.insert_temp(id, now));
            ui.ctx().request_discard("Arcade layout measured a row");
            // If the pass budget is exhausted, finish centring on a new frame.
            ui.ctx().request_repaint();
        }
        inner
    }

    fn row_text(&self, ui: &mut Ui, title: &str, description: &str) {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add(Label::new(RichText::new(title).font(super::font_weight(text::BODY.size, 500)).color(colour(self.palette.text))).truncate())
            .on_hover_text(title);
        if !description.is_empty() {
            ui.add(Label::new(RichText::new(description).font(super::font(text::SMALL)).color(colour(self.palette.muted))).wrap());
        }
    }

    /// The whole row toggles, including its label. Keyboard Space/Enter work
    /// through egui's normal click sense; disabled rows cannot change state.
    pub fn switch_row(&self, ui: &mut Ui, value: &mut bool, title: &str, description: &str) -> Response {
        let enabled = ui.is_enabled();
        let row = self.row(ui, title, description, |ui| self.switch(ui, value, title));
        let mut switch = row.inner;
        // Registered after the switch, so a click anywhere on the row lands here once.
        let area = ui.interact(row.response.rect, switch.id.with("row"), Sense::CLICK);
        if area.clicked() && enabled {
            *value = !*value;
            switch.mark_changed();
        }
        switch | area
    }

    pub fn switch(&self, ui: &mut Ui, value: &mut bool, label: &str) -> Response {
        let (rect, mut r) = ui.allocate_exact_size(Vec2::new(40.0, size::CONTROL_SM), Sense::click());
        if r.clicked() && ui.is_enabled() {
            *value = !*value;
            r.mark_changed();
        }
        r.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *value, label));
        let track = Rect::from_center_size(rect.center(), Vec2::new(32.0, 18.0));
        // Off uses muted rather than CSS line-strong so the track keeps 3:1.
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

    /// `.segmented`: options on a sunken track; the chosen one is a raised panel.
    pub fn segmented<T: Copy + PartialEq>(&self, ui: &mut Ui, value: &mut T, choices: &[(T, &str)]) -> Response {
        let p = self.palette;
        let enabled = ui.is_enabled();
        let font = super::font(text::CONTROL);
        Frame::new()
            .fill(colour(p.sunken))
            .stroke(Stroke::new(1.0, colour(p.line)))
            .corner_radius(radius::TILE)
            .inner_margin(3.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.horizontal(|ui| {
                    let mut combined: Option<Response> = None;
                    for &(option, label) in choices {
                        let active = *value == option;
                        let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label.to_string(), font.clone(), Color32::PLACEHOLDER));
                        let (rect, mut r) = ui.allocate_exact_size(Vec2::new(galley.size().x + 24.0, size::CONTROL_SM), Sense::click());
                        if r.clicked() && !active && enabled {
                            *value = option;
                            r.mark_changed();
                        }
                        if active {
                            ui.painter().rect(rect, 6.0, colour(p.panel), Stroke::new(1.0, colour(p.line)), StrokeKind::Inside);
                        }
                        let fg = if enabled && (active || r.hovered()) { p.text } else { p.muted };
                        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, colour(fg));
                        r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, enabled, active, label));
                        self.focus(ui, &r);
                        combined = Some(match combined {
                            Some(all) => all | r,
                            None => r,
                        });
                    }
                    combined.unwrap_or_else(|| ui.allocate_response(Vec2::ZERO, Sense::hover()))
                })
                .inner
            })
            .inner
    }

    pub fn text_input(&self, ui: &mut Ui, value: &mut String, hint: &str, invalid: Option<&str>) -> Response {
        let r = ui.add_sized(
            [ui.available_width(), size::CONTROL],
            TextEdit::singleline(value)
                .font(super::font(text::CONTROL))
                .margin(Margin::symmetric(10, 0))
                .vertical_align(Align::Center)
                .hint_text(RichText::new(hint).color(colour(self.palette.muted))),
        );
        self.focus(ui, &r);
        if let Some(message) = invalid {
            ui.painter().rect_stroke(r.rect, radius::SM, Stroke::new(1.0, colour(self.palette.danger)), StrokeKind::Inside);
            ui.add(Label::new(RichText::new(message).font(super::font(text::SMALL)).color(colour(self.ink.danger))).wrap());
        }
        r
    }

    /// `.select`: 32 px high like the other controls, with the kit chevron.
    pub fn select<T: PartialEq + Clone>(&self, ui: &mut Ui, id: impl std::hash::Hash, value: &mut T, options: &[(T, &str)]) -> Response {
        let selected = options.iter().find(|(v, _)| v == value).map_or("Choose…", |(_, label)| *label);
        let before = value.clone();
        let kit = *self;
        let mut r = ui
            .scope(|ui| {
                ui.spacing_mut().interact_size.y = size::CONTROL;
                ui.spacing_mut().button_padding = Vec2::new(10.0, 8.0);
                ::egui::ComboBox::from_id_salt(id)
                    .selected_text(RichText::new(selected).font(super::font(text::CONTROL)))
                    .width(ui.available_width().min(240.0))
                    .truncate()
                    .icon(move |ui, rect, _, _| kit.paint_icon(ui, "chevronDown", rect.center(), colour(kit.palette.muted)))
                    .show_ui(ui, |ui| {
                        for (option, label) in options {
                            ui.selectable_value(value, option.clone(), *label);
                        }
                    })
                    .response
            })
            .inner;
        if before != *value {
            r.mark_changed();
        }
        self.focus(ui, &r);
        r
    }

    /// `.badge`: a 20 px pill; neutral badges carry a hairline.
    pub fn badge(&self, ui: &mut Ui, label: &str, tone: Tone) -> Response {
        let (fg, bg) = self.tones(tone);
        Frame::new()
            .fill(bg)
            .stroke(if tone == Tone::Neutral { Stroke::new(1.0, colour(self.palette.line)) } else { Stroke::NONE })
            .corner_radius(radius::PILL)
            .inner_margin(Margin::symmetric(7, 3))
            .show(ui, |ui| ui.add(Label::new(RichText::new(label).font(super::font(text::CAPTION)).color(fg)).truncate()).on_hover_text(label))
            .inner
    }

    /// `.key-chip`: 22 px high, at least as wide, with a 2 px bottom edge.
    pub fn key_chip(&self, ui: &mut Ui, key: &str) -> Response {
        let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(key.to_string(), super::font(text::CAPTION), colour(self.palette.text)));
        let size = Vec2::new((galley.size().x + 12.0).max(size::KEY_CHIP), size::KEY_CHIP);
        let (rect, r) = ui.allocate_exact_size(size, Sense::hover());
        let edge = Stroke::new(1.0, colour(self.palette.line_strong));
        ui.painter().rect(rect, radius::XS, colour(self.palette.sunken), edge, StrokeKind::Inside);
        ui.painter().hline(rect.x_range().shrink(radius::XS), rect.bottom() - 1.5, edge);
        ui.painter().galley(rect.center() - galley.size() * 0.5 - Vec2::new(0.0, 0.5), galley, colour(self.palette.text));
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, key));
        r
    }

    pub fn notice(&self, ui: &mut Ui, message: &str, tone: Tone) -> Response {
        self.notice_action(ui, message, tone, |_| ()).response
    }

    /// `.notice`: a tone icon and body-colour text on the tone's soft fill, with
    /// any action at the trailing edge.
    pub fn notice_action<R>(&self, ui: &mut Ui, message: &str, tone: Tone, action: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let (fg, bg) = self.tones(tone);
        let p = self.palette;
        let (fill, border) = match tone {
            Tone::Warn => (bg, colour(p.line).lerp_to_gamma(colour(p.warn), 0.35)),
            Tone::Danger => (bg, colour(p.line).lerp_to_gamma(colour(p.danger), 0.35)),
            _ => (colour(p.panel), colour(p.line)),
        };
        let id = ui.id().with(("arcade-notice", message));
        Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1.0, border))
            .corner_radius(radius::MD)
            .inner_margin(Margin { left: ROW_PAD as i8, right: ROW_PAD as i8, top: 10, bottom: 10 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.beside(
                    ui,
                    id,
                    0.0,
                    |ui| {
                        ui.horizontal_top(|ui| {
                            let (icon, _) = ui.allocate_exact_size(Vec2::splat(size::ICON), Sense::hover());
                            self.paint_icon(ui, tone_icon(tone), icon.center(), fg);
                            ui.add(Label::new(RichText::new(message).font(super::font(text::CONTROL)).color(colour(p.text))).wrap());
                        });
                    },
                    action,
                )
            })
    }

    /// The empty state (`.empty-state`): dashed outline, icon, title, text, actions.
    pub fn empty_state<R>(&self, ui: &mut Ui, icon: &str, title: &str, description: &str, actions: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let r = Frame::new().corner_radius(radius::MD).inner_margin(24.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(size::TILE), Sense::hover());
                ui.painter().rect(r, radius::TILE, colour(self.palette.sunken), Stroke::new(1.0, colour(self.palette.line)), StrokeKind::Inside);
                self.paint_icon(ui, icon, r.center(), colour(self.palette.muted));
                ui.add_space(4.0);
                ui.label(RichText::new(title).font(super::font(text::HEADING)).color(colour(self.palette.text)));
                if !description.is_empty() {
                    ui.add(Label::new(RichText::new(description).font(super::font(text::SMALL)).color(colour(self.palette.muted))).wrap());
                }
                ui.add_space(6.0);
                actions(ui)
            })
            .inner
        });
        let rect = r.response.rect;
        let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
        ui.painter().extend(::egui::Shape::dashed_line(&corners, Stroke::new(1.0, colour(self.palette.line_strong)), 5.0, 4.0));
        r
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

    /// `.glyph-tile`: a 36 px `sunken` tile with a `line` edge and an 18 px
    /// glyph in `text`. `glyph` is a white mask texture (an Arcade app's
    /// glyph); without one, `icon` names a kit icon.
    pub fn glyph_tile(&self, ui: &mut Ui, glyph: Option<::egui::TextureId>, icon: &str) -> Response {
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(size::TILE), Sense::hover());
        let p = self.palette;
        ui.painter().rect(rect, radius::TILE, colour(p.sunken), Stroke::new(1.0, colour(p.line)), StrokeKind::Inside);
        match glyph {
            Some(texture) => {
                let glyph = Rect::from_center_size(rect.center(), Vec2::splat(size::ICON_TILE));
                ui.painter().image(texture, glyph, Rect::from_min_max(::egui::pos2(0.0, 0.0), ::egui::pos2(1.0, 1.0)), colour(p.text));
            }
            None => self.paint_icon_sized(ui, icon, rect.center(), size::ICON_TILE, colour(p.text)),
        }
        r
    }

    /// A [`Kit::row`] with a glyph tile before its text (`.row` with a
    /// leading `.glyph-tile`), as the Connected apps page lists apps.
    pub fn row_with_glyph<R>(
        &self,
        ui: &mut Ui,
        glyph: Option<::egui::TextureId>,
        icon: &str,
        title: &str,
        description: &str,
        control: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        self.row_divider(ui);
        // A label is not an identity: shortcut sheets and data lists repeat names.
        let id = ui.next_auto_id().with(("arcade-row", title));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(10.0);
            let inner = self.beside(
                ui,
                id,
                size::ROW - 20.0,
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        self.glyph_tile(ui, glyph, icon);
                        let width = ui.available_width();
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            self.row_text(ui, title, description);
                        });
                    });
                },
                control,
            );
            ui.add_space(10.0);
            inner
        })
    }

    /// A checkbox (`input[type=checkbox]`, `.check-row`): selection in lists
    /// and dialog options; settings use [`Kit::switch_row`]. The label is
    /// part of the hit target.
    pub fn checkbox(&self, ui: &mut Ui, value: &mut bool, label: &str) -> Response {
        let enabled = ui.is_enabled();
        let font = super::font(text::CONTROL);
        let p = self.palette;
        let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label.to_string(), font, colour(p.text)));
        let height = size::HIT_TARGET.max(galley.size().y);
        let (rect, mut r) = ui.allocate_exact_size(Vec2::new(15.0 + 8.0 + galley.size().x, height), Sense::click());
        if r.clicked() && enabled {
            *value = !*value;
            r.mark_changed();
        }
        r.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *value, label));
        let boxed = Rect::from_min_size(::egui::pos2(rect.left(), rect.center().y - 7.5), Vec2::splat(15.0));
        let (fill, edge) = match (enabled, *value) {
            (false, _) => (p.sunken, p.line_strong),
            (true, true) => (p.accent, p.accent),
            (true, false) => (p.panel, if r.hovered() { p.muted } else { p.line_strong }),
        };
        ui.painter().rect(boxed, 4.0, colour(fill), Stroke::new(1.0, colour(edge)), StrokeKind::Inside);
        if *value {
            let ink = if enabled { p.accent_text } else { p.muted };
            self.paint_icon_sized(ui, "check", boxed.center(), 12.0, colour(ink));
        }
        let fg = if enabled { p.text } else { p.muted };
        ui.painter().galley(::egui::pos2(boxed.right() + 8.0, rect.center().y - galley.size().y * 0.5), galley, colour(fg));
        if r.has_focus() {
            ui.painter().rect_stroke(boxed.expand(size::FOCUS_OFFSET), 6.0, Stroke::new(size::FOCUS_RING, colour(p.accent)), StrokeKind::Outside);
        }
        if r.hovered() && enabled {
            ui.ctx().set_cursor_icon(::egui::CursorIcon::PointingHand);
        }
        r
    }

    /// A dialog's title (`title-sm`) and body text (muted, wrapping).
    pub fn dialog_head(&self, ui: &mut Ui, title: &str, body: &str) {
        ui.add(Label::new(RichText::new(title).font(super::font(text::TITLE_SM)).color(colour(self.palette.text))).wrap());
        if !body.is_empty() {
            ui.add_space(8.0);
            ui.add(Label::new(RichText::new(body).font(super::font(text::BODY)).color(colour(self.palette.muted))).wrap());
        }
    }

    /// The common onboarding hero: 52 px mark, display title and one sentence.
    /// Apps supply up to three steps and keep Skip in their fixed page actions.
    pub fn welcome_head(&self, ui: &mut Ui, glyph: Option<::egui::TextureId>, icon: &str, title: &str, body: &str) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size::TILE_LG), Sense::hover());
        ui.painter().rect(rect, 13, colour(self.palette.accent_soft), Stroke::NONE, StrokeKind::Inside);
        match glyph {
            Some(texture) => {
                let mark = Rect::from_center_size(rect.center(), Vec2::splat(size::ICON_TILE_LARGE));
                ui.painter().image(texture, mark, Rect::from_min_max(::egui::pos2(0.0, 0.0), ::egui::pos2(1.0, 1.0)), colour(self.ink.accent));
            }
            None => self.paint_icon_sized(ui, icon, rect.center(), size::ICON_TILE_LARGE, colour(self.ink.accent)),
        }
        ui.add_space(16.0);
        ui.add(Label::new(RichText::new(title).font(super::font(text::DISPLAY)).color(colour(self.palette.text))).wrap());
        ui.add_space(8.0);
        ui.add(Label::new(RichText::new(body).font(super::font(text::BODY)).color(colour(self.palette.muted))).wrap());
    }

    /// A keyboard-operable disclosure with the kit's section heading style.
    pub fn disclosure<R>(&self, ui: &mut Ui, id: impl std::hash::Hash, title: &str, body: impl FnOnce(&mut Ui) -> R) -> ::egui::CollapsingResponse<R> {
        ::egui::CollapsingHeader::new(RichText::new(title).font(super::font(text::HEADING)).color(colour(self.palette.text))).id_salt(id).show(ui, body)
    }

    /// `.dialog-actions`: buttons right-aligned in the order given (primary
    /// last), 8 px apart, 20 px below the content. When they don't fit on one
    /// line they wrap, still in order.
    pub fn dialog_actions<R>(&self, ui: &mut Ui, actions: impl FnOnce(&mut Ui) -> R) -> R {
        ui.add_space(20.0);
        let id = ui.id().with("arcade-dialog-actions");
        let width = ui.available_width();
        let known = ui.data(|d| d.get_temp::<f32>(id));
        let r = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(w) = known.filter(|w| *w <= width) {
                ui.add_space((width - w).max(0.0));
            }
            let start = ui.cursor().left();
            let inner = actions(ui);
            (inner, ui.min_rect().right() - start)
        });
        let (inner, used) = r.inner;
        if known.is_none_or(|w| (w - used).abs() > 0.5) && used <= width {
            ui.data_mut(|d| d.insert_temp(id, used));
            ui.ctx().request_discard("Arcade layout measured dialog actions");
        }
        inner
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
        self.paint_icon_sized(ui, name, center, size::ICON, color);
    }

    /// [`Kit::paint_icon`] at another kit icon size (15 small, 18 in tiles).
    pub fn paint_icon_sized(&self, ui: &Ui, name: &str, center: ::egui::Pos2, px: f32, color: Color32) {
        let Some(icon) = crate::design::icon(name) else { return };
        let scale = px / 24.0;
        let origin = center - Vec2::splat(px * 0.5);
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
        let frame = Frame::new().fill(colour(self.palette.bg)).inner_margin(Margin { left: 12, right: 12, top: 16, bottom: 14 });
        ::egui::SidePanel::left("arcade-sidebar").exact_width(width).resizable(false).frame(frame).show(ctx, |ui| {
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
            ui.add_space(24.0);
            ui.spacing_mut().item_spacing.y = 2.0;
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
                // `.tab-button`: muted until hovered or active; the active item is a
                // raised panel with an accent icon.
                if active {
                    ui.painter().rect(rect, radius::SM, colour(self.palette.panel), Stroke::new(1.0, colour(self.palette.line)), StrokeKind::Inside);
                } else if r.hovered() {
                    ui.painter().rect_filled(rect, radius::SM, colour(self.palette.hover));
                }
                let fg = if active || r.hovered() { self.palette.text } else { self.palette.muted };
                let icon_x = if collapsed { rect.center().x } else { rect.left() + 18.0 };
                self.paint_icon(ui, item.icon, ::egui::pos2(icon_x, rect.center().y), colour(if active { self.ink.accent } else { fg }));
                if !collapsed {
                    let mut right = rect.right() - 8.0;
                    if let Some(count) = item.count {
                        let text = crate::design::format::format_count(count.min(i64::MAX as usize) as i64);
                        let galley =
                            ui.fonts_mut(|fonts| fonts.layout_no_wrap(text, super::font_weight(text::CAPTION.size, 600), colour(self.palette.accent_text)));
                        let pill = Rect::from_min_max(
                            ::egui::pos2(right - (galley.size().x + 12.0).max(18.0), rect.center().y - 9.0),
                            ::egui::pos2(right, rect.center().y + 9.0),
                        );
                        ui.painter().rect_filled(pill, radius::PILL, colour(self.palette.accent));
                        ui.painter().galley(pill.center() - galley.size() * 0.5, galley, colour(self.palette.accent_text));
                        right = pill.left() - 8.0;
                    }
                    let mut job = ::egui::text::LayoutJob::simple_singleline(item.label.to_string(), super::font_weight(text::BODY.size, 500), colour(fg));
                    job.wrap.max_width = (right - rect.left() - 36.0).max(0.0);
                    job.wrap.max_rows = 1;
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    ui.painter().galley(::egui::pos2(rect.left() + 36.0, rect.center().y - galley.size().y * 0.5), galley, colour(fg));
                }
                r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), active, item.label));
                self.focus(ui, &r);
                r = r.on_hover_text(item.label);
                if i == *selected && old != *selected {
                    r.request_focus();
                }
            }
            // `.sidebar-foot` / `.sidebar-meta`: "Version" and the number; collapsed, the number only.
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                let meta = |text: &str| RichText::new(text).font(super::font(text::CAPTION)).color(colour(self.palette.muted));
                ui.horizontal(|ui| {
                    if collapsed {
                        ui.add(Label::new(meta(version)).selectable(true));
                    } else {
                        ui.add_space(8.0);
                        ui.label(meta("Version"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(8.0);
                            ui.add(Label::new(meta(version)).selectable(true));
                        });
                    }
                });
                ui.add_space(12.0);
                let y = ui.cursor().bottom();
                ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, colour(self.palette.line)));
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
        // `.toast`: a raised panel at the bottom right with a tone icon, the
        // message and a close button at the trailing edge.
        let p = kit.palette;
        let (fg, _) = kit.tones(self.tone);
        ::egui::Area::new(Id::new("arcade-toast")).anchor(Align2::RIGHT_BOTTOM, [-20.0, -20.0]).order(::egui::Order::Foreground).show(ctx, |ui| {
            let width = 380.0_f32.min(ctx.content_rect().width() - 40.0).max(0.0);
            Frame::new()
                .fill(colour(p.panel))
                .stroke(Stroke::new(1.0, colour(p.line)))
                .corner_radius(radius::MD)
                .shadow(ui.visuals().popup_shadow)
                .inner_margin(Margin { left: ROW_PAD as i8, right: 12, top: 10, bottom: 10 })
                .show(ui, |ui| {
                    ui.set_width(width - 26.0);
                    ui.horizontal_top(|ui| {
                        let (icon, _) = ui.allocate_exact_size(Vec2::splat(size::ICON), Sense::hover());
                        kit.paint_icon(ui, tone_icon(self.tone), icon.center(), fg);
                        let text = (ui.available_width() - size::CONTROL_SM - ui.spacing().item_spacing.x).max(0.0);
                        ui.allocate_ui_with_layout(Vec2::new(text, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(text);
                            ui.add(Label::new(RichText::new(&self.message).font(super::font(text::CONTROL)).color(colour(p.text))).wrap());
                        });
                        if kit.icon_button(ui, "x", "Close notification").clicked() {
                            open = false;
                        }
                    });
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
    fn welcome_and_disclosures_fit_the_minimum_window_at_large_text() {
        for theme in [Theme::Light, Theme::Dark] {
            let ctx = Context::default();
            super::super::install_inter(&ctx);
            super::super::apply(&ctx, if theme.is_dark() { "dark" } else { "light" }, "arcade.find");
            ctx.set_zoom_factor(1.25);
            for _ in 0..5 {
                let _ = ctx.run(
                    ::egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(::egui::Pos2::ZERO, Vec2::new(720.0 / 1.25, 520.0 / 1.25))),
                        ..Default::default()
                    },
                    |ctx| {
                        let kit = Kit::for_theme(theme, "arcade.find");
                        kit.page(ctx, |ui| {
                            kit.welcome_head(ui, None, "search", "Arcade Find", "Press a shortcut, type, and any file or folder shows up.");
                            let response = kit.disclosure(ui, "help", "How it works", |ui| {
                                kit.path(ui, "/a/long/folder/name");
                            });
                            assert!(response.header_response.rect.right() <= ui.max_rect().right() + 1.0);
                            assert!(ui.min_rect().right() <= ui.max_rect().right() + 1.0);
                        });
                    },
                );
            }
        }
    }

    #[test]
    fn repeated_row_titles_keep_independent_measurements() {
        for theme in [Theme::Light, Theme::Dark] {
            let ctx = Context::default();
            super::super::install_inter(&ctx);
            super::super::apply(&ctx, if theme.is_dark() { "dark" } else { "light" }, "arcade.find");
            ctx.set_zoom_factor(1.25);
            let kit = Kit::for_theme(theme, "arcade.find");
            for frame in 0..8 {
                let output = ctx.run(input(720.0 / 1.25, 520.0 / 1.25, vec![]), |ctx| {
                    kit.page_with_head(ctx, "Shortcuts", "Keys in the search.", |ui| {
                        kit.list(ui, |ui| {
                            for keys in [&["Esc"][..], &["Ctrl", "Enter"][..], &["Ctrl", "Shift", "Enter"][..]] {
                                kit.row(ui, "Return to search results", "", |ui| {
                                    for key in keys {
                                        kit.key_chip(ui, key);
                                    }
                                });
                            }
                        });
                        kit.list(ui, |ui| {
                            for label in ["Open", "Show in folder"] {
                                kit.row_with_glyph(ui, None, "folder", "Same folder name", "Its own independent row.", |ui| {
                                    kit.button(ui, label, ButtonKind::Default);
                                });
                            }
                        });
                    });
                });
                if frame >= 4 {
                    assert!(!output.platform_output.requested_discard(), "{theme:?}, frame {frame}: duplicate titles must not fight over cached sizes");
                }
            }
        }
    }

    #[test]
    fn disabled_row_text_keeps_readable_rendered_contrast() {
        fn luminance(rgb: [f32; 3]) -> f32 {
            let c = rgb.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
            c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
        }
        for theme in [Theme::Light, Theme::Dark] {
            let ctx = Context::default();
            super::super::install_inter(&ctx);
            super::super::apply(&ctx, if theme.is_dark() { "dark" } else { "light" }, "arcade.find");
            let kit = Kit::for_theme(theme, "arcade.find");
            let mut value = false;
            for _ in 0..4 {
                let output = ctx.run(input(520.0, 300.0, vec![]), |ctx| {
                    ::egui::CentralPanel::default().show(ctx, |ui| {
                        kit.list(ui, |ui| {
                            ui.add_enabled_ui(false, |ui| {
                                kit.switch_row(ui, &mut value, "Start at login", "Install Find to start it at login.");
                            });
                        });
                    });
                });
                let bg = crate::design::rgb(kit.palette.panel).map(|v| v as f32 / 255.0);
                let background = luminance(bg);
                let mut glyph_vertices = 0;
                for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
                    if let ::egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        for vertex in mesh.vertices.iter().filter(|v| v.uv != ::egui::epaint::WHITE_UV) {
                            let [r, g, b, alpha] = vertex.color.to_srgba_unmultiplied();
                            if alpha == 0 {
                                continue;
                            }
                            glyph_vertices += 1;
                            let a = alpha as f32 / 255.0;
                            let fg = [r, g, b].map(|v| v as f32 / 255.0);
                            let blended = std::array::from_fn(|i| fg[i] * a + bg[i] * (1.0 - a));
                            let foreground = luminance(blended);
                            let ratio = (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                            assert!(ratio >= 4.5, "{theme:?}: disabled text paints at {ratio:.2}:1");
                        }
                    }
                }
                assert!(glyph_vertices > 0, "inspect real rendered glyph vertices");
            }
        }
    }

    #[test]
    fn glyph_row_names_ellipsize_and_keep_wrapping_paths_in_the_row() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        super::super::apply(&ctx, "light", "arcade.find");
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let name = "a-long-folder-name-".repeat(7);
        let path = format!("/home/me/Documents/{name}");
        for _ in 0..4 {
            let mut row = Rect::NOTHING;
            let output = ctx.run(input(440.0, 520.0, vec![]), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    row = kit
                        .list(ui, |ui| {
                            kit.row_with_glyph(ui, None, "folder", &name, &path, |ui| {
                                kit.button_state(ui, "Remove", ButtonKind::Ghost, true, false, true);
                            })
                        })
                        .inner
                        .response
                        .rect;
                });
            });
            let mut names = 0;
            let mut paths = 0;
            for shape in &output.shapes {
                if let ::egui::Shape::Text(text) = &shape.shape {
                    if text.galley.job.text == name {
                        names += 1;
                        assert_eq!(text.galley.rows.len(), 1, "a long folder name uses one line");
                        assert!(text.galley.elided, "the full name remains available through its tooltip");
                    }
                    if text.galley.job.text == path {
                        paths += 1;
                        assert!(text.galley.rows.len() > 1, "the path wraps");
                        assert!(text.pos.y + text.galley.size().y <= row.bottom() - 8.0, "the wrapped path stays above the divider");
                    }
                }
            }
            assert_eq!((names, paths), (1, 1));
        }
    }

    #[test]
    fn fixed_page_head_is_outside_the_body_scroll_clip() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        super::super::apply(&ctx, "dark", "arcade.find");
        let kit = Kit::for_theme(Theme::Dark, "arcade.find");
        let mut body = Rect::NOTHING;
        for _ in 0..4 {
            let output = ctx.run(input(720.0, 520.0, vec![]), |ctx| {
                kit.page_with_head(ctx, "Locations", "Choose the folders to index.", |ui| {
                    body = ui.clip_rect();
                    for n in 0..20 {
                        kit.row(ui, &format!("Folder {n}"), "/home/me/Documents", |_| ());
                    }
                });
            });
            let title = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    ::egui::Shape::Text(text) if text.galley.job.text == "Locations" => Some(text),
                    _ => None,
                })
                .expect("the fixed heading is painted");
            assert!(title.pos.y + title.galley.size().y < body.top(), "the scroll clip starts below the fixed page heading");
            assert!(body.bottom() <= 520.0, "only the body can scroll outside its content bounds");
        }
    }

    #[test]
    fn changing_description_never_paints_across_the_next_divider() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        super::super::apply(&ctx, "light", "arcade.find");
        // A frame must be safe even when egui has used its other layout passes.
        ctx.options_mut(|options| options.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let long = "App, version, desktop and Link details for a bug report. No file names.";
        for frame in 0..8 {
            let description = if frame < 4 { "" } else { long };
            let mut row = Rect::NOTHING;
            let output = ctx.run(input(420.0, 300.0, vec![]), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    kit.list(ui, |ui| {
                        row = kit
                            .row(ui, "Diagnostics", description, |ui| kit.button_state(ui, "Copy diagnostics", ButtonKind::Default, true, false, true))
                            .response
                            .rect;
                        kit.row(ui, "Licences", "Third-party licences ship with the app.", |_| ());
                    });
                });
            });
            if !description.is_empty() {
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        ::egui::Shape::Text(text) if text.galley.job.text == description => Some(text),
                        _ => None,
                    })
                    .expect("the description is drawn");
                assert!(text.pos.y + text.galley.size().y <= row.bottom() - 8.0, "frame {frame}: changed description must fit its row, above its divider");
            }
        }
    }

    #[test]
    fn wrapped_description_stays_above_the_next_row_divider_after_resize() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        super::super::apply(&ctx, "light", "arcade.find");
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let description = "App, version, desktop and Link details for a bug report. No file names.";
        let mut selected = 0;
        ctx.set_zoom_factor(1.25);
        for width in [720.0, 760.0, 720.0, 760.0] {
            for _ in 0..8 {
                let mut row = Rect::NOTHING;
                let output = ctx.run(input(width / 1.25, 560.0 / 1.25, vec![]), |ctx| {
                    kit.sidebar(ctx, "Arcade Find", "0.5.0", &[NavItem { label: "About", icon: "info", count: None }], &mut selected);
                    kit.page(ctx, |ui| {
                        kit.page_head(ui, "About", "Version, updates, help and licences.");
                        kit.list(ui, |ui| {
                            kit.row_with_glyph(ui, None, "search", "Version 0.4.3", "Press a shortcut, type, and any file or folder shows up.", |ui| {
                                kit.badge(ui, "Not installed", Tone::Neutral);
                            });
                            kit.row(ui, "Updates", "Install Arcade Tools to get updates automatically.", |ui| {
                                kit.button_state(ui, "Check for updates", ButtonKind::Default, true, false, true)
                            });
                            kit.row(ui, "User guide", "How to search, filter and use Find with other apps.", |ui| {
                                kit.button_state(ui, "Open user guide", ButtonKind::Default, true, false, true)
                            });
                            kit.row(ui, "Logs", "/tmp/arcade-e2e-profile/find/data/logs", |ui| {
                                kit.button_state(ui, "Open logs folder", ButtonKind::Default, true, false, true)
                            });
                            row = kit
                                .row(ui, "Diagnostics", description, |ui| kit.button_state(ui, "Copy diagnostics", ButtonKind::Default, true, false, true))
                                .response
                                .rect;
                            kit.row(ui, "Licences", "Third-party licences ship with the app.", |ui| {
                                kit.button_state(ui, "Open", ButtonKind::Default, true, false, true)
                            });
                        });
                    });
                });
                let mut matched = 0;
                for shape in &output.shapes {
                    if let ::egui::Shape::Text(text) = &shape.shape {
                        if text.galley.job.text == description {
                            matched += 1;
                            let bottom = text.pos.y + text.galley.size().y;
                            assert!(bottom <= row.bottom() - 8.0, "description crosses the next divider: bottom {bottom}, row {row:?}");
                        }
                    }
                }
                assert_eq!(matched, 1, "inspect the actual description geometry");
            }
        }
    }

    #[test]
    fn long_glyph_rows_settle_without_repeated_discard() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        super::super::apply(&ctx, "light", "arcade.find");
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let path = format!("/tmp/arcade-e2e-profile/{}", "a-long-folder-name-".repeat(7));
        let mut pending = true;
        let mut selected = 0;
        ctx.set_zoom_factor(1.25);
        for frame in 0..8 {
            let output = ctx.run(input(760.0 / 1.25, 560.0 / 1.25, vec![]), |ctx| {
                kit.sidebar(ctx, "Arcade Find", "0.5.0", &[NavItem { label: "Locations", icon: "folder", count: None }], &mut selected);
                kit.page(ctx, |ui| {
                    kit.page_head(ui, "Locations", "The folders Find indexes, and what it skips.");
                    kit.section_head(ui, "Folders to index");
                    kit.list(ui, |ui| {
                        kit.row_with_glyph(ui, None, "folder", &path, &path, |ui| {
                            kit.button_state(ui, "Remove", ButtonKind::Ghost, false, false, true);
                        });
                    });
                    kit.section_head(ui, "Skip");
                    kit.list(ui, |ui| {
                        for i in 0..10 {
                            kit.row(ui, &format!("Excluded folder {i}"), "Skipped with everything inside it.", |ui| {
                                kit.button_state(ui, "Remove", ButtonKind::Ghost, true, false, true);
                            });
                        }
                    });
                });
            });
            pending = output.platform_output.requested_discard();
            if frame >= 4 {
                assert!(!pending, "frame {frame}: a stable long glyph row must stop requesting layout passes");
            }
        }
        assert!(!pending);
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
                                    kit.row_with_glyph(
                                        ui,
                                        None,
                                        "apps",
                                        "Arcade Shelf",
                                        "Drag results onto Shelf to keep them at hand while you work.",
                                        |ui| {
                                            kit.badge(ui, "Not running", Tone::Neutral);
                                            kit.button_state(ui, "Open", ButtonKind::Default, true, false, true);
                                        },
                                    );
                                });
                                let mut ticked = true;
                                kit.checkbox(ui, &mut ticked, "Remove the downloaded file");
                                kit.dialog_head(ui, "Install Arcade Find?", "Arcade Find goes in your applications folder and can start at login.");
                                kit.dialog_actions(ui, |ui| {
                                    kit.button(ui, "Don't ask again for this file", ButtonKind::Ghost);
                                    kit.button(ui, "Just run it", ButtonKind::Default);
                                    kit.button(ui, "Install", ButtonKind::Primary);
                                });
                                ui.horizontal_wrapped(|ui| {
                                    for kind in [ButtonKind::Default, ButtonKind::Primary, ButtonKind::Ghost, ButtonKind::Danger, ButtonKind::Destructive] {
                                        kit.button(ui, "Rescan the whole index", kind);
                                    }
                                    kit.button_state(ui, "Rescanning", ButtonKind::Primary, true, true, false);
                                    kit.button_state(ui, "Disabled", ButtonKind::Default, false, false, true);
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
    fn rows_keep_control_order_beside_the_text_and_busy_buttons_do_not_move_neighbours() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let appearance = 0;
        let mut centred = (Rect::NOTHING, Rect::NOTHING);
        let mut rects = (Vec::new(), Rect::NOTHING, Rect::NOTHING, Rect::NOTHING);
        // The first pass measures each row's control; the second lays it out.
        for _ in 0..3 {
            let _ = ctx.run(input(720.0, 520.0, vec![]), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    let row = kit.list(ui, |ui| {
                        kit.row(ui, "Theme", "Follow the system, or always use light or dark", |ui| {
                            let mut parts = Vec::new();
                            for (value, label) in [(0, "System"), (1, "Light"), (2, "Dark")] {
                                parts.push(kit.button(ui, label, if appearance == value { ButtonKind::Primary } else { ButtonKind::Default }).rect);
                            }
                            parts
                        })
                    });
                    let plain = kit.list(ui, |ui| kit.row(ui, "Index folder", "", |ui| kit.button(ui, "Copy path", ButtonKind::Default).rect));
                    centred = (plain.inner.response.rect, plain.inner.inner);
                    ui.horizontal(|ui| {
                        let busy = kit.button_state(ui, "Rescanning", ButtonKind::Primary, true, true, false).rect;
                        let next = kit.button(ui, "Small", ButtonKind::Default).rect;
                        rects = (row.inner.inner.clone(), row.response.rect, busy, next);
                    });
                });
            });
        }
        let (parts, list, busy, next) = rects;
        assert!(parts.windows(2).all(|pair| pair[0].right() <= pair[1].left()), "controls keep their left-to-right order: {parts:?}");
        assert!(parts.iter().all(|part| part.right() <= list.right() + 0.5), "the control stays inside the list");
        assert!(parts[0].left() > list.left() + list.width() * 0.3, "a narrow control sits beside the text, not under it");
        assert!(next.left() >= busy.right(), "a busy button must not overlap the next widget");
        let (row, control) = centred;
        assert!((row.center().y - control.center().y).abs() < 1.0, "a row's control is centred on the row: {row:?} {control:?}");
    }

    #[test]
    fn glyph_rows_put_the_tile_before_the_text_and_the_control_beside_it() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        let kit = Kit::for_theme(Theme::Dark, "arcade.find");
        let mut got = (Rect::NOTHING, Vec::new(), Rect::NOTHING);
        for _ in 0..3 {
            let _ = ctx.run(input(720.0, 520.0, vec![]), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    let list = kit.list(ui, |ui| {
                        kit.row_with_glyph(ui, None, "apps", "Arcade Shelf", "Keep files at hand while you work.", |ui| {
                            vec![kit.badge(ui, "Connected", Tone::Ok).rect, kit.button_state(ui, "Open", ButtonKind::Default, true, false, true).rect]
                        })
                    });
                    got = (list.inner.response.rect, list.inner.inner, list.response.rect);
                });
            });
        }
        let (row, controls, list) = got;
        assert!(row.height() >= size::ROW - 0.5, "a glyph row keeps the row height: {row:?}");
        assert!(controls[0].right() <= controls[1].left(), "controls keep their order");
        assert!(controls[1].right() <= list.right() + 0.5 && controls[0].left() > list.center().x, "controls sit at the trailing edge");
        assert!((controls[1].center().y - row.center().y).abs() < 1.0, "controls are centred on the row");
    }

    #[test]
    fn checkbox_label_toggles_and_disabled_does_not() {
        let ctx = Context::default();
        super::super::install_inter(&ctx);
        let kit = Kit::for_theme(Theme::Light, "arcade.find");
        let mut value = false;
        let draw = |events, enabled, value: &mut bool| {
            let mut out = (Rect::NOTHING, false);
            let _ = ctx.run(input(460.0, 300.0, events), |ctx| {
                ::egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let r = kit.checkbox(ui, value, "Start at login");
                        out = (r.rect, r.changed());
                    });
                });
            });
            out
        };
        let (rect, _) = draw(vec![], true, &mut value);
        assert!(rect.height() >= size::HIT_TARGET, "a 28 px hit target");
        let on_label = ::egui::pos2(rect.right() - 4.0, rect.center().y);
        let (_, changed) = draw(click(on_label), true, &mut value);
        assert!(changed && value, "clicking the label ticks the box");
        let (_, changed) = draw(click(on_label), false, &mut value);
        assert!(!changed && value, "a disabled checkbox keeps its state");
    }

    #[test]
    fn dialog_actions_align_right_in_order_and_wrap_when_narrow() {
        for width in [460.0, 260.0] {
            let ctx = Context::default();
            super::super::install_inter(&ctx);
            let kit = Kit::for_theme(Theme::Light, "arcade.find");
            let mut got = (Vec::new(), Rect::NOTHING);
            for _ in 0..3 {
                let _ = ctx.run(input(width, 300.0, vec![]), |ctx| {
                    ::egui::CentralPanel::default().show(ctx, |ui| {
                        let area = ui.max_rect();
                        let buttons = kit.dialog_actions(ui, |ui| {
                            ["Don't ask again for this file", "Just run it", "Install"].map(|l| kit.button(ui, l, ButtonKind::Default).rect).to_vec()
                        });
                        got = (buttons, area);
                    });
                });
            }
            let (buttons, area) = got;
            assert!(buttons.iter().all(|b| b.right() <= area.right() + 0.5 && b.left() >= area.left() - 0.5), "{width}: inside the dialog");
            let last = buttons.last().unwrap();
            if width > 400.0 {
                assert!(buttons.windows(2).all(|w| w[0].right() < w[1].left()), "in order on one line");
                assert!(area.right() - last.right() < 1.0, "the primary action ends at the trailing edge: {area:?} {buttons:?}");
            } else {
                assert!(buttons.windows(2).all(|w| w[0].top() < w[1].top() || w[0].right() < w[1].left()), "wrapped, still in order");
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
