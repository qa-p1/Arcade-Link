//! Link file kinds as the file managers name them: MIME types (desktop entries, Dolphin, Nemo) and extensions
//! (Windows, Thunar). The extensions come from Link's own table, so a kind matches the same files everywhere.

/// Extensions Link maps to each kind (lower case, no dot), the inverse of `content::file_kind_for_extension`.
const EXTENSIONS: &[(&str, &[&str])] = &[
    (
        "image",
        &[
            "png", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "avif", "ico", "jxl", "tga", "qoi", "psd", "raw", "cr2",
            "nef", "dng", "arw", "exr", "hdr",
        ],
    ),
    ("svg", &["svg", "svgz"]),
    ("video", &["mp4", "mkv", "mov", "webm", "avi", "m4v", "wmv", "flv", "mpg", "mpeg", "3gp", "ogv"]),
    ("audio", &["mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "wma", "aiff", "aif", "alac", "mid", "midi"]),
    ("pdf", &["pdf"]),
    ("document", &["doc", "docx", "odt", "rtf", "pages"]),
    ("ebook", &["epub", "mobi", "azw3", "fb2", "djvu"]),
    ("spreadsheet", &["xls", "xlsx", "ods", "numbers"]),
    ("csv", &["csv", "tsv"]),
    ("presentation", &["ppt", "pptx", "odp", "key"]),
    ("archive", &["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "zst", "lz", "lzma", "cab", "iso"]),
    ("text", &["txt", "log", "ini", "cfg", "conf", "nfo"]),
    ("markdown", &["md", "markdown", "mdown", "mkd"]),
    ("json", &["json", "jsonl", "ndjson", "geojson"]),
    ("html", &["html", "htm", "xhtml"]),
    (
        "code",
        &[
            "rs", "c", "h", "cpp", "hpp", "cc", "py", "js", "mjs", "ts", "tsx", "jsx", "java", "kt", "go", "rb", "php", "swift", "cs", "sh", "bash", "zsh",
            "fish", "ps1", "yaml", "yml", "toml", "xml", "css", "scss", "sql", "lua", "dart", "qml", "vue", "svelte",
        ],
    ),
    ("subtitle", &["srt", "vtt", "ass", "ssa", "sub", "sbv", "lrc"]),
    ("font", &["ttf", "otf", "woff", "woff2", "ttc"]),
    ("model", &["obj", "stl", "gltf", "glb", "fbx", "3mf", "dae", "ply"]),
];

/// MIME types for each kind. `image/*` style wildcards are not used: not every file manager honors them.
const MIME: &[(&str, &[&str])] = &[
    (
        "image",
        &[
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
            "image/bmp",
            "image/tiff",
            "image/heic",
            "image/heif",
            "image/avif",
            "image/x-icon",
            "image/vnd.microsoft.icon",
            "image/jxl",
            "image/x-tga",
            "image/vnd.adobe.photoshop",
            "image/x-dcraw",
            "image/x-canon-cr2",
            "image/x-nikon-nef",
            "image/x-adobe-dng",
            "image/x-sony-arw",
            "image/x-exr",
            "image/x-hdr",
            "image/qoi",
        ],
    ),
    ("svg", &["image/svg+xml", "image/svg+xml-compressed"]),
    (
        "video",
        &[
            "video/mp4",
            "video/x-matroska",
            "video/quicktime",
            "video/webm",
            "video/x-msvideo",
            "video/x-m4v",
            "video/x-ms-wmv",
            "video/x-flv",
            "video/mpeg",
            "video/3gpp",
            "video/ogg",
        ],
    ),
    (
        "audio",
        &[
            "audio/mpeg",
            "audio/x-wav",
            "audio/wav",
            "audio/flac",
            "audio/x-flac",
            "audio/ogg",
            "audio/x-vorbis+ogg",
            "audio/x-opus+ogg",
            "audio/opus",
            "audio/mp4",
            "audio/x-m4a",
            "audio/aac",
            "audio/x-ms-wma",
            "audio/x-aiff",
            "audio/midi",
        ],
    ),
    ("pdf", &["application/pdf"]),
    (
        "document",
        &[
            "application/msword",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "application/vnd.oasis.opendocument.text",
            "application/rtf",
            "text/rtf",
            "application/vnd.apple.pages",
        ],
    ),
    ("ebook", &["application/epub+zip", "application/x-mobipocket-ebook", "application/vnd.amazon.ebook", "application/x-fictionbook+xml", "image/vnd.djvu"]),
    (
        "spreadsheet",
        &[
            "application/vnd.ms-excel",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "application/vnd.oasis.opendocument.spreadsheet",
            "application/vnd.apple.numbers",
        ],
    ),
    ("csv", &["text/csv", "text/tab-separated-values"]),
    (
        "presentation",
        &[
            "application/vnd.ms-powerpoint",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "application/vnd.oasis.opendocument.presentation",
            "application/vnd.apple.keynote",
        ],
    ),
    (
        "archive",
        &[
            "application/zip",
            "application/x-tar",
            "application/gzip",
            "application/x-compressed-tar",
            "application/x-bzip2",
            "application/x-xz",
            "application/x-7z-compressed",
            "application/vnd.rar",
            "application/x-rar",
            "application/zstd",
            "application/x-lzip",
            "application/x-lzma",
            "application/vnd.ms-cab-compressed",
            "application/x-cd-image",
        ],
    ),
    ("text", &["text/plain"]),
    ("markdown", &["text/markdown", "text/x-markdown"]),
    ("json", &["application/json", "application/x-ndjson", "application/geo+json"]),
    ("html", &["text/html", "application/xhtml+xml"]),
    (
        "code",
        &[
            "text/x-rust",
            "text/x-csrc",
            "text/x-chdr",
            "text/x-c++src",
            "text/x-c++hdr",
            "text/x-python",
            "text/x-python3",
            "application/javascript",
            "text/javascript",
            "application/x-typescript",
            "text/x-java",
            "text/x-kotlin",
            "text/x-go",
            "application/x-ruby",
            "application/x-php",
            "text/x-swift",
            "text/x-csharp",
            "application/x-shellscript",
            "text/x-sh",
            "application/x-yaml",
            "application/toml",
            "application/xml",
            "text/xml",
            "text/css",
            "text/x-scss",
            "application/sql",
            "text/x-lua",
            "text/x-dart",
            "text/x-qml",
        ],
    ),
    ("subtitle", &["application/x-subrip", "text/vtt", "text/x-ssa", "text/x-microdvd"]),
    ("font", &["font/ttf", "font/otf", "font/woff", "font/woff2", "font/collection", "application/x-font-ttf"]),
    ("model", &["model/obj", "model/stl", "model/gltf+json", "model/gltf-binary", "model/3mf", "model/vnd.collada+xml"]),
];

/// The kinds an accept pattern takes: `file/image` → image and svg (a refinement), `file/*` → every kind.
pub fn kinds_for(accept: &str) -> Vec<&'static str> {
    let pattern = accept.split(";hint=").next().unwrap_or("").trim().trim_end_matches("[]");
    if pattern == "*" || pattern == "file/*" || pattern == "file/any" {
        return EXTENSIONS.iter().map(|(k, _)| *k).collect();
    }
    let Some(kind) = pattern.strip_prefix("file/") else { return Vec::new() };
    EXTENSIONS.iter().map(|(k, _)| *k).filter(|k| arcade_link::content::is_kind(&format!("file/{k}"), &format!("file/{kind}"))).collect()
}

/// Whether the accepts take every file (so menus can use "all files" instead of a list).
pub fn takes_any_file(accepts: &[String]) -> bool {
    accepts.iter().any(|a| matches!(a.split(";hint=").next().unwrap_or("").trim().trim_end_matches("[]"), "*" | "file/*" | "file/any"))
}

fn collect(accepts: &[String], table: &[(&str, &[&'static str])]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for a in accepts {
        for kind in kinds_for(a) {
            if let Some((_, items)) = table.iter().find(|(k, _)| *k == kind) {
                for i in *items {
                    if !out.contains(i) {
                        out.push(i);
                    }
                }
            }
        }
    }
    out
}

pub fn mime_types(accepts: &[String]) -> Vec<&'static str> {
    collect(accepts, MIME)
}

pub fn extensions(accepts: &[String]) -> Vec<&'static str> {
    collect(accepts, EXTENSIONS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_table_is_links_own() {
        for (kind, exts) in EXTENSIONS {
            for e in *exts {
                assert_eq!(arcade_link::content::file_kind_for_extension(e), *kind, ".{e}");
            }
        }
        assert!(MIME.iter().all(|(k, _)| EXTENSIONS.iter().any(|(e, _)| e == k)));
    }

    #[test]
    fn patterns_cover_their_refinements() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(extensions(&s(&["file/image"])).contains(&"svg"));
        assert!(!extensions(&s(&["file/svg"])).contains(&"png"));
        assert!(extensions(&s(&["file/spreadsheet[]"])).contains(&"csv"));
        assert!(mime_types(&s(&["file/pdf"])) == ["application/pdf"]);
        assert!(takes_any_file(&s(&["text/plain", "file/*[]"])) && !takes_any_file(&s(&["file/image"])));
        assert!(extensions(&s(&["text/url"])).is_empty());
    }
}
