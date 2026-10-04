//! The shared content vocabulary (SPEC §5): what a value is and how it travels.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Inline text above this size goes in a handoff file instead.
pub const INLINE_TEXT_LIMIT: usize = 256 * 1024;

/// One input or output value.
///
/// `type` is a Link content type (`text/plain`, `file/image`, `folder/reference`,
/// `structured/color`, `screen/region`, …). Files travel as `path`; text as
/// `text` (plus `html` for `text/rich`); structured values as `data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Content {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// For file arrays (`file/<kind>[]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Semantic detail the type alone loses, e.g. `["command"]` for text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// The app that created a handoff file; it deletes the file, not the receiver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// A display name, e.g. a suggested file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl Content {
    pub fn text(kind: &str, text: impl Into<String>) -> Content {
        Content { kind: kind.into(), text: Some(text.into()), ..Default::default() }
    }

    pub fn plain(text: impl Into<String>) -> Content {
        Content::text("text/plain", text)
    }

    pub fn url(url: impl Into<String>) -> Content {
        Content::text("text/url", url)
    }

    /// A file by path, typed from its extension (`file/image`, `file/pdf`, …),
    /// or `folder/reference` for a directory.
    pub fn file(path: &Path) -> Content {
        let kind = if path.is_dir() { "folder/reference".to_string() } else { file_type_for_path(path) };
        let size = if path.is_file() { std::fs::metadata(path).ok().map(|m| m.len()) } else { None };
        Content { kind, path: Some(path.to_string_lossy().into_owned()), size, ..Default::default() }
    }

    /// Several files as one `file/<kind>[]` value (`file/any[]` if mixed).
    pub fn files(paths: &[&Path]) -> Content {
        let kinds: Vec<&str> = paths.iter().map(|p| file_kind_for_path(p)).collect();
        let kind = match kinds.first() {
            Some(k) if kinds.iter().all(|x| x == k) => *k,
            _ => "any",
        };
        Content { kind: format!("file/{kind}[]"), paths: paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(), ..Default::default() }
    }

    pub fn structured(name: &str, data: Value) -> Content {
        Content { kind: format!("structured/{name}"), data: Some(data), ..Default::default() }
    }

    pub fn with_hint(mut self, hint: &str) -> Content {
        self.hints.push(hint.into());
        self
    }

    pub fn with_owner(mut self, owner: &str) -> Content {
        self.owner = Some(owner.into());
        self
    }

    pub fn has_hint(&self, hint: &str) -> bool {
        self.hints.iter().any(|h| h == hint)
    }

    /// Every path this value refers to.
    pub fn all_paths(&self) -> Vec<&str> {
        self.path.iter().map(String::as_str).chain(self.paths.iter().map(String::as_str)).collect()
    }

    /// The size used for limit checks: the file sizes, or the text length.
    pub fn byte_size(&self) -> u64 {
        if let Some(s) = self.size {
            return s;
        }
        let files: u64 = self.all_paths().iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
        files + self.text.as_ref().map_or(0, |t| t.len() as u64) + self.html.as_ref().map_or(0, |t| t.len() as u64)
    }
}

/// The family of a content type: `text`, `file`, `folder`, `structured`, `screen`.
pub fn family(kind: &str) -> &str {
    kind.split('/').next().unwrap_or("")
}

fn is_array(kind: &str) -> bool {
    kind.ends_with("[]")
}

fn base(kind: &str) -> &str {
    kind.strip_suffix("[]").unwrap_or(kind)
}

/// Splits an accept pattern into its type and a required hint:
/// `text/plain;hint=command` → (`text/plain`, Some(`command`)).
fn split_hint(accept: &str) -> (&str, Option<&str>) {
    match accept.split_once(";hint=") {
        Some((t, h)) => (t.trim(), Some(h.trim())),
        None => (accept.trim(), None),
    }
}

/// Whether a value of type `offered` satisfies the accept pattern `accept`.
///
/// - `*` accepts anything; `text/*`, `file/*`, `structured/*` accept their family.
/// - `text/url` and `text/rich` also satisfy `text/plain` (both carry plain text).
/// - `file/any` accepts any single file, like `file/*`.
/// - An array pattern (`file/image[]`, `file/*[]`) also accepts a single file
///   (a batch of one). A single-file pattern never accepts an array.
pub fn type_matches(accept: &str, offered: &str) -> bool {
    let (accept, _) = split_hint(accept);
    if accept == "*" || accept == offered {
        return true;
    }
    if is_array(offered) && !is_array(accept) {
        return false;
    }
    let (a, o) = (base(accept), base(offered));
    if a == o {
        return true;
    }
    let (af, of) = (family(a), family(o));
    if af != of {
        return false;
    }
    let a_sub = a.split_once('/').map_or("", |x| x.1);
    let o_sub = o.split_once('/').map_or("", |x| x.1);
    match af {
        "text" => a_sub == "*" || (a_sub == "plain" && (o_sub == "url" || o_sub == "rich")),
        "file" => a_sub == "*" || a_sub == "any",
        "structured" | "screen" => a_sub == "*",
        _ => false,
    }
}

/// Whether `content` satisfies `accept`, including a required hint.
pub fn content_matches(accept: &str, content: &Content) -> bool {
    let (_, hint) = split_hint(accept);
    type_matches(accept, &content.kind) && hint.is_none_or(|h| content.has_hint(h))
}

/// Whether any of `accepts` takes a value of type `offered`.
pub fn accepts_type(accepts: &[String], offered: &str) -> bool {
    accepts.iter().any(|a| type_matches(a, offered))
}

/// Whether any of `accepts` takes `content`.
pub fn accepts_content(accepts: &[String], content: &Content) -> bool {
    accepts.iter().any(|a| content_matches(a, content))
}

/// The Link file kind for a path, from its extension. Unknown → `any`.
pub fn file_kind_for_path(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    file_kind_for_extension(&ext)
}

/// The Link file kind for a lower-case extension without the dot.
pub fn file_kind_for_extension(ext: &str) -> &'static str {
    match ext {
        "png" | "jpg" | "jpeg" | "jpe" | "jfif" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "avif" | "ico" | "svg" | "jxl" | "tga"
        | "qoi" | "psd" | "raw" | "cr2" | "nef" | "dng" | "arw" | "exr" | "hdr" => "image",
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" | "3gp" | "ogv" => "video",
        "mp3" | "wav" | "flac" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "wma" | "aiff" | "aif" | "alac" | "mid" | "midi" => "audio",
        "pdf" => "pdf",
        "doc" | "docx" | "odt" | "rtf" | "pages" | "epub" => "document",
        "xls" | "xlsx" | "ods" | "csv" | "tsv" | "numbers" => "spreadsheet",
        "ppt" | "pptx" | "odp" | "key" => "presentation",
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz" | "lzma" | "cab" | "iso" => "archive",
        "txt" | "md" | "markdown" | "log" | "ini" | "cfg" | "conf" | "nfo" => "text",
        "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "java" | "kt" | "go" | "rb" | "php" | "swift" | "cs" | "sh"
        | "bash" | "zsh" | "fish" | "ps1" | "json" | "yaml" | "yml" | "toml" | "xml" | "html" | "htm" | "css" | "scss" | "sql" | "lua" | "dart" | "qml"
        | "vue" | "svelte" => "code",
        "ttf" | "otf" | "woff" | "woff2" | "ttc" => "font",
        "obj" | "stl" | "gltf" | "glb" | "fbx" | "3mf" | "dae" | "ply" => "model",
        _ => "any",
    }
}

/// `file/<kind>` for a path.
pub fn file_type_for_path(path: &Path) -> String {
    format!("file/{}", file_kind_for_path(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_rules() {
        assert!(type_matches("file/*", "file/image"));
        assert!(!type_matches("file/*", "file/image[]"));
        assert!(type_matches("file/*[]", "file/image[]"));
        assert!(type_matches("file/*[]", "file/image"));
        assert!(type_matches("text/plain", "text/url"));
        assert!(!type_matches("text/url", "text/plain"));
        assert!(type_matches("text/*", "text/rich"));
        assert!(!type_matches("file/image", "file/pdf"));
        assert!(type_matches("file/any", "file/pdf"));
        assert!(!type_matches("folder/reference", "file/any"));
    }

    #[test]
    fn hints_are_required_when_named() {
        let cmd = Content::plain("rm -rf build").with_hint("command");
        assert!(content_matches("text/plain;hint=command", &cmd));
        assert!(!content_matches("text/plain;hint=command", &Content::plain("hello")));
    }
}
