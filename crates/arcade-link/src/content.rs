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
/// `text` (plus `html` for `text/rich`); structured values as `data`. Large
/// text travels in a handoff file in `path`, and its HTML in `htmlPath`.
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
    /// `text/rich` HTML handed off as a file, beside the text in `path`.
    #[serde(rename = "htmlPath", default, skip_serializing_if = "Option::is_none")]
    pub html_path: Option<String>,
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
        let html_file = self.html_path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        files + html_file + self.text.as_ref().map_or(0, |t| t.len() as u64) + self.html.as_ref().map_or(0, |t| t.len() as u64)
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
        "text" => a_sub == "*" || (a_sub == "plain" && matches!(o_sub, "url" | "rich" | "markdown")),
        "file" => a_sub == "*" || a_sub == "any" || ancestors(o).any(|t| t == a),
        "structured" | "screen" => a_sub == "*",
        _ => false,
    }
}

/// The broader type a refined type also is (SPEC §5.1): `file/csv` is a `file/spreadsheet`, `file/json` and
/// `file/html` are `file/code`, `file/svg` is a `file/image`, `file/ebook` is a `file/document`, `file/markdown` is a
/// `file/text`, `text/markdown` is `text/plain`. An action that accepts the broader type takes the refined one, so
/// cards written before a kind was refined keep working. Arrays keep their `[]`.
pub fn parent_type(kind: &str) -> Option<String> {
    let array = is_array(kind);
    let parent = match base(kind) {
        "file/csv" => "file/spreadsheet",
        "file/json" | "file/html" => "file/code",
        "file/svg" => "file/image",
        "file/ebook" => "file/document",
        "file/markdown" => "file/text",
        "text/markdown" => "text/plain",
        _ => return None,
    };
    Some(if array { format!("{parent}[]") } else { parent.to_string() })
}

/// `kind` itself is not included; its parent, the parent's parent, and so on.
fn ancestors(kind: &str) -> impl Iterator<Item = String> {
    std::iter::successors(parent_type(kind), |k| parent_type(k))
}

/// Whether `kind` is `of` or a refinement of it (`is_kind("file/csv", "file/spreadsheet")` is true).
pub fn is_kind(kind: &str, of: &str) -> bool {
    kind == of || ancestors(kind).any(|k| k == of)
}

/// The type to send to an action that accepts `accepts`: `offered` if one of the patterns names it (or a wildcard
/// takes it), otherwise the closest broader type a pattern names. A refined kind is only sent to an app whose card
/// names it, so an app built before the kind existed still recognizes what it receives.
pub fn type_for(accepts: &[String], offered: &str) -> String {
    // A pattern names `t` when it is `t` itself (or `t`'s array form, which takes a single value too), or a
    // wildcard (`*`, `<family>/*`, `file/any`) that takes it.
    let named = |t: &str| {
        accepts.iter().any(|a| {
            let (a, _) = split_hint(a);
            let sub = base(a).split_once('/').map_or("", |x| x.1);
            a == "*" || (base(a) == base(t) || sub == "*" || sub == "any") && type_matches(a, t)
        })
    };
    std::iter::once(offered.to_string()).chain(ancestors(offered)).find(|t| named(t)).unwrap_or_else(|| offered.to_string())
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
        "svg" | "svgz" => "svg",
        "csv" | "tsv" => "csv",
        "json" | "jsonl" | "ndjson" | "geojson" => "json",
        "html" | "htm" | "xhtml" => "html",
        "md" | "markdown" | "mdown" | "mkd" => "markdown",
        "epub" | "mobi" | "azw3" | "fb2" | "djvu" => "ebook",
        "srt" | "vtt" | "ass" | "ssa" | "sub" | "sbv" | "lrc" => "subtitle",
        "png" | "jpg" | "jpeg" | "jpe" | "jfif" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "avif" | "ico" | "jxl" | "tga" | "qoi"
        | "psd" | "raw" | "cr2" | "nef" | "dng" | "arw" | "exr" | "hdr" => "image",
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" | "3gp" | "ogv" => "video",
        "mp3" | "wav" | "flac" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "wma" | "aiff" | "aif" | "alac" | "mid" | "midi" => "audio",
        "pdf" => "pdf",
        "doc" | "docx" | "odt" | "rtf" | "pages" => "document",
        "xls" | "xlsx" | "ods" | "numbers" => "spreadsheet",
        "ppt" | "pptx" | "odp" | "key" => "presentation",
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz" | "lzma" | "cab" | "iso" => "archive",
        "txt" | "log" | "ini" | "cfg" | "conf" | "nfo" => "text",
        "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "java" | "kt" | "go" | "rb" | "php" | "swift" | "cs" | "sh"
        | "bash" | "zsh" | "fish" | "ps1" | "yaml" | "yml" | "toml" | "xml" | "css" | "scss" | "sql" | "lua" | "dart" | "qml" | "vue" | "svelte" => "code",
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
    fn refined_kinds_match_their_broader_type() {
        assert!(type_matches("file/spreadsheet", "file/csv"));
        assert!(type_matches("file/spreadsheet[]", "file/csv[]"));
        assert!(type_matches("file/spreadsheet[]", "file/csv"));
        assert!(!type_matches("file/spreadsheet", "file/csv[]"));
        assert!(!type_matches("file/csv", "file/spreadsheet"));
        assert!(type_matches("file/code", "file/json"));
        assert!(type_matches("file/image", "file/svg"));
        assert!(type_matches("text/plain", "text/markdown"));
        assert!(!type_matches("file/image", "file/subtitle"));
        assert!(type_matches("file/any", "file/subtitle"));
    }

    #[test]
    fn a_refined_kind_is_sent_as_what_the_card_names() {
        let old_card = vec!["file/spreadsheet".to_string()];
        assert_eq!(type_for(&old_card, "file/csv"), "file/spreadsheet");
        let new_card = vec!["file/csv".to_string(), "file/spreadsheet".to_string()];
        assert_eq!(type_for(&new_card, "file/csv"), "file/csv");
        let any = vec!["file/*".to_string()];
        assert_eq!(type_for(&any, "file/csv"), "file/csv");
        assert_eq!(type_for(&["file/spreadsheet[]".to_string()], "file/csv[]"), "file/spreadsheet[]");
        assert_eq!(type_for(&["file/spreadsheet[]".to_string()], "file/csv"), "file/spreadsheet");
        assert_eq!(type_for(&["text/plain".to_string()], "text/markdown"), "text/plain");
        assert_eq!(type_for(&["file/image".to_string()], "file/pdf"), "file/pdf");
        assert!(is_kind("file/csv", "file/spreadsheet") && !is_kind("file/spreadsheet", "file/csv"));
    }

    #[test]
    fn hints_are_required_when_named() {
        let cmd = Content::plain("rm -rf build").with_hint("command");
        assert!(content_matches("text/plain;hint=command", &cmd));
        assert!(!content_matches("text/plain;hint=command", &Content::plain("hello")));
    }
}
