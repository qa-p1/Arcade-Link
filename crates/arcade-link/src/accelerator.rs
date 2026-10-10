//! Canonical key notation shared by manifests, shortcut docs and Qt (SPEC §8.5).
//! Parsing is lenient; files must store the canonical result. No OS registration
//! happens here. `Super` always means Cmd on macOS, Win on Windows.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Linux,
    Windows,
    Macos,
}

impl Platform {
    pub const ALL: [Self; 3] = [Self::Linux, Self::Windows, Self::Macos];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Macos => "macos",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorError(pub String);
impl fmt::Display for AcceleratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AcceleratorError {}

const MODIFIERS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Super"];
const KEYS: &[&str] = &[
    "Space",
    "Enter",
    "Tab",
    "Escape",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Up",
    "Down",
    "Left",
    "Right",
    "Minus",
    "Equal",
    "BracketLeft",
    "BracketRight",
    "Backslash",
    "Semicolon",
    "Quote",
    "Backquote",
    "Comma",
    "Period",
    "Slash",
    "Print",
    "Pause",
    "ScrollLock",
    "CapsLock",
    "NumLock",
    "Menu",
    "NumpadAdd",
    "NumpadSubtract",
    "NumpadMultiply",
    "NumpadDivide",
    "NumpadDecimal",
    "NumpadEnter",
    "VolumeUp",
    "VolumeDown",
    "VolumeMute",
    "MediaPlayPause",
    "MediaNext",
    "MediaPrevious",
    "MediaStop",
    "BrightnessUp",
    "BrightnessDown",
    "MouseLeft",
    "MouseRight",
    "MouseMiddle",
    "MouseBack",
    "MouseForward",
    "WheelUp",
    "WheelDown",
];

fn modifier(s: &str) -> Option<usize> {
    match s.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => Some(0),
        "alt" | "option" | "opt" => Some(1),
        "shift" => Some(2),
        "super" | "cmd" | "command" | "win" | "windows" | "meta" | "logo" | "mod4" | "super_l" => Some(3),
        _ => None,
    }
}

fn key(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "/" => "Slash",
        "-" => "Minus",
        "=" | "plus" => "Equal",
        "," => "Comma",
        "." => "Period",
        ";" => "Semicolon",
        "'" | "apostrophe" => "Quote",
        "`" | "grave" => "Backquote",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "\\" => "Backslash",
        "return" => "Enter",
        "esc" => "Escape",
        "del" => "Delete",
        "ins" => "Insert",
        "pgup" | "prior" => "PageUp",
        "pgdn" | "next" => "PageDown",
        "arrowup" => "Up",
        "arrowdown" => "Down",
        "arrowleft" => "Left",
        "arrowright" => "Right",
        "xf86audioraisevolume" => "VolumeUp",
        "xf86audiolowervolume" => "VolumeDown",
        "xf86audiomute" => "VolumeMute",
        "xf86audioplay" => "MediaPlayPause",
        "xf86audionext" => "MediaNext",
        "xf86audioprev" => "MediaPrevious",
        "xf86audiostop" => "MediaStop",
        "xf86monbrightnessup" => "BrightnessUp",
        "xf86monbrightnessdown" => "BrightnessDown",
        "mouse:272" => "MouseLeft",
        "mouse:273" => "MouseRight",
        "mouse:274" => "MouseMiddle",
        "mouse:275" => "MouseBack",
        "mouse:276" => "MouseForward",
        "mouse_up" => "WheelUp",
        "mouse_down" => "WheelDown",
        _ => "",
    };
    if !alias.is_empty() {
        return Some(alias.into());
    }
    if let Some(k) = KEYS.iter().find(|k| k.eq_ignore_ascii_case(s)) {
        return Some((*k).into());
    }
    if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() {
        return Some(s.to_ascii_uppercase());
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=24).contains(n)) {
        return Some(format!("F{n}"));
    }
    if let Some(n) = lower.strip_prefix("numpad").filter(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit()) {
        return Some(format!("Numpad{n}"));
    }
    let raw = lower.strip_prefix("code:").or_else(|| lower.strip_prefix("code"))?;
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Keycodes are decimal strings; normalize without a platform-sized integer.
    let raw = raw.trim_start_matches('0');
    Some(format!("Code{}", if raw.is_empty() { "0" } else { raw }))
}

/// Lenient input to canonical `Ctrl+Alt+Shift+Super+Key` chords, separated
/// by a single space. A single modifier is a valid tap chord. Empty input,
/// repeated modifiers, ambiguous `++`, and unknown keys are errors.
pub fn normalize(input: &str) -> Result<String, AcceleratorError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AcceleratorError("empty accelerator".into()));
    }
    // Whitespace around '+' is harmless, whitespace between chords is meaningful.
    let compact = input.split('+').map(str::trim).collect::<Vec<_>>().join("+");
    let mut out = Vec::new();
    for chord in compact.split_whitespace() {
        let mut mods = [false; 4];
        let mut main = None;
        for part in chord.split('+') {
            if let Some(i) = modifier(part) {
                if mods[i] {
                    return Err(AcceleratorError(format!("repeated modifier in {chord:?}")));
                }
                mods[i] = true;
            } else {
                if main.is_some() {
                    return Err(AcceleratorError(format!("more than one key in {chord:?}")));
                }
                main = Some(key(part).ok_or_else(|| AcceleratorError(format!("unknown key {part:?}")))?);
            }
        }
        let mut parts: Vec<String> = MODIFIERS.iter().enumerate().filter(|(i, _)| mods[*i]).map(|(_, m)| (*m).into()).collect();
        if let Some(k) = main {
            parts.push(k);
        } else if parts.len() != 1 {
            return Err(AcceleratorError(format!("a chord needs a key: {chord:?}")));
        }
        out.push(parts.join("+"));
    }
    Ok(out.join(" "))
}

pub fn is_canonical(input: &str) -> bool {
    normalize(input).is_ok_and(|n| n == input)
}

fn friendly(key: &str, os: Platform) -> &str {
    match key {
        "Slash" => "/",
        "Minus" => "-",
        "Equal" => "=",
        "Comma" => ",",
        "Period" => ".",
        "Semicolon" => ";",
        "Quote" => "'",
        "Backquote" => "`",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Up" => "↑",
        "Down" => "↓",
        "Left" => "←",
        "Right" => "→",
        "Enter" if os == Platform::Macos => "↩",
        "Tab" if os == Platform::Macos => "⇥",
        "Escape" if os == Platform::Macos => "⎋",
        "Backspace" if os == Platform::Macos => "⌫",
        "Delete" if os == Platform::Macos => "⌦",
        _ => key,
    }
}

/// OS-facing display only; never write display strings into a shortcut file.
pub fn display(input: &str, os: Platform) -> Result<String, AcceleratorError> {
    Ok(normalize(input)?
        .split(' ')
        .map(|chord| {
            let parts: Vec<&str> = chord
                .split('+')
                .map(|part| match (part, os) {
                    ("Ctrl", Platform::Macos) => "⌃",
                    ("Alt", Platform::Macos) => "⌥",
                    ("Shift", Platform::Macos) => "⇧",
                    ("Super", Platform::Macos) => "⌘",
                    ("Super", Platform::Windows) => "Win",
                    _ => friendly(part, os),
                })
                .collect();
            parts.join(if os == Platform::Macos { "" } else { "+" })
        })
        .collect::<Vec<_>>()
        .join(" "))
}

/// Equal sequences and strict sequence prefixes conflict. Invalid inputs
/// are errors rather than silently looking like an unused shortcut.
pub fn conflicts(a: &str, b: &str) -> Result<bool, AcceleratorError> {
    let a = normalize(a)?;
    let b = normalize(b)?;
    Ok(a == b || a.strip_prefix(&b).is_some_and(|s| s.starts_with(' ')) || b.strip_prefix(&a).is_some_and(|s| s.starts_with(' ')))
}
