//! Turning a menu click into Link requests, and putting results where the user wants them (PLAN §4.4, items 4 and
//! 6). The owning app still checks every request (SPEC §1.5); results it writes to its own folder are copied out,
//! never moved, and never over an existing file.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use arcade_link::content::{self, Content};
use arcade_link::manifest::{output, Action};
use arcade_link::offers::{input_type, Item, Prefs};
use arcade_link::wire::InvokeRequest;

/// Where a request came from; decides the default output place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    FileMenu,
    Browser,
}

impl Origin {
    pub fn source(self) -> &'static str {
        match self {
            Origin::FileMenu => "file-manager",
            Origin::Browser => "browser",
        }
    }
}

/// `<app>/<action>` split into app, action and Box preset (`box:x#webp`).
pub fn parse_reference(reference: &str) -> Option<(&str, &str, Option<&str>)> {
    let (app, action) = reference.split_once('/')?;
    if app.is_empty() || action.is_empty() {
        return None;
    }
    Some(match action.split_once('#') {
        Some((a, p)) => (app, a, Some(p)),
        None => (app, action, None),
    })
}

/// A file's Link content.
pub fn file_content(path: &Path) -> Content {
    if path.is_dir() {
        let mut c = Content { kind: "folder/reference".into(), ..Content::default() };
        c.path = Some(path.to_string_lossy().into_owned());
        return c;
    }
    let mut c = Content::file(path);
    c.size = std::fs::metadata(path).ok().map(|m| m.len());
    c
}

/// The inputs of each run for `paths`: one run with an array when the action takes several at once, else one run
/// per file.
pub fn runs_for(action: &Action, paths: &[PathBuf]) -> Vec<Vec<Content>> {
    let contents: Vec<Content> = paths.iter().map(|p| file_content(p)).collect();
    if contents.len() > 1 {
        let items: Vec<Item> = contents.iter().map(|c| Item::new(&c.kind)).collect();
        if let Some(array) = input_type(&items).filter(|t| action.accepts.iter().any(|a| content::type_matches(a, t))) {
            let mut c = Content { kind: array, ..Content::default() };
            c.paths = contents.iter().filter_map(|c| c.path.clone()).collect();
            return vec![vec![c]];
        }
    }
    contents.into_iter().map(|c| vec![c]).collect()
}

/// The request for one run: the action's form defaults under the caller's values (checked by the form).
pub fn request(reference: &str, action: &Action, inputs: Vec<Content>, values: &Map<String, Value>, origin: Origin) -> Result<InvokeRequest, String> {
    let (_, id, preset) = parse_reference(reference).ok_or("not an action reference")?;
    let options = match &action.options {
        Some(form) => form.resolve(values)?,
        None => values.clone(),
    };
    let mut r = InvokeRequest::new(id, origin.source());
    r.preset = preset.map(str::to_string);
    r.version = Some(action.version);
    r.inputs = inputs;
    r.options = Value::Object(options);
    r.context.interactive = action.interactive;
    r.context.reason = "Run from the Arcade menu".into();
    Ok(r)
}

/// Where results go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Folder(PathBuf),
    /// Ask the user (the progress window shows a folder picker).
    Ask,
    /// Shown or copied, not saved.
    Keep,
}

/// The user's Downloads folder.
pub fn downloads_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // xdg-user-dirs: XDG_DOWNLOAD_DIR="$HOME/Downloads"
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
            for line in text.lines() {
                if let Some(v) = line.trim().strip_prefix("XDG_DOWNLOAD_DIR=") {
                    let v = v.trim_matches('"').replace("$HOME", &home.to_string_lossy());
                    if Path::new(&v).is_absolute() {
                        return PathBuf::from(v);
                    }
                }
            }
        }
    }
    home.join("Downloads")
}

/// The target for a run: the user's choice first (ask, or one folder for everything), then the action's `output`,
/// then the origin's default (next to the input for the file menu, Downloads for the browser).
pub fn target(action: &Action, prefs: &Prefs, origin: Origin, first_input: Option<&Path>) -> Target {
    if action.output.as_deref() == Some(output::NONE) {
        return Target::Keep;
    }
    if prefs.ask_where || action.output.as_deref() == Some(output::ASK) {
        return Target::Ask;
    }
    if !prefs.output_folder.is_empty() {
        return Target::Folder(PathBuf::from(&prefs.output_folder));
    }
    let beside = first_input.and_then(|p| p.parent()).filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf);
    match (action.output.as_deref(), origin, beside) {
        (Some(output::DOWNLOADS), _, _) | (_, Origin::Browser, _) | (_, _, None) => Target::Folder(downloads_dir()),
        (_, Origin::FileMenu, Some(dir)) => Target::Folder(dir),
    }
}

/// A free name in `dir` for `name`: `photo.webp`, else `photo (1).webp`, `photo (2).webp`, …
pub fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).expect("some name is free")
}

/// What a finished run left for the user.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Placed {
    pub files: Vec<PathBuf>,
    pub texts: Vec<String>,
}

/// Copies each file output into `dir` (unless the app already wrote it there) and collects text outputs.
pub fn place(outputs: &[Content], dir: Option<&Path>) -> std::io::Result<Placed> {
    let mut placed = Placed::default();
    for c in outputs {
        let sources: Vec<PathBuf> = c.path.iter().chain(&c.paths).map(PathBuf::from).collect();
        if c.kind.starts_with("file/") || c.kind == "folder/reference" {
            for src in sources {
                let Some(dir) = dir else {
                    placed.files.push(src);
                    continue;
                };
                if src.parent() == Some(dir) || c.kind == "folder/reference" {
                    placed.files.push(src);
                    continue;
                }
                let name = c.name.clone().or_else(|| src.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| "result".into());
                std::fs::create_dir_all(dir)?;
                let dest = free_name(dir, &name);
                std::fs::copy(&src, &dest)?;
                placed.files.push(dest);
            }
        } else if let Some(t) = &c.text {
            placed.texts.push(t.clone());
        } else if let Ok(t) = arcade_link::handoff::read_text(c) {
            placed.texts.push(t);
        }
    }
    Ok(placed)
}

/// How far a job of `total` runs is while run `index` reports `current`. An app with no number for its step
/// (looking up a video, joining files) sends none: the first run then has none either, so the bar moves instead
/// of sitting at 0; later runs still count the ones that finished.
pub fn overall(index: usize, total: f32, current: Option<f32>) -> Option<f32> {
    let total = total.max(1.0);
    match current {
        Some(fraction) => Some((index as f32 + fraction.clamp(0.0, 1.0)) / total),
        None if index > 0 => Some(index as f32 / total),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_number_from_the_app_stays_no_number_until_a_run_has_finished() {
        assert_eq!(overall(0, 1.0, None), None);
        assert_eq!(overall(0, 1.0, Some(0.36)), Some(0.36));
        assert_eq!(overall(0, 1.0, Some(2.0)), Some(1.0));
        assert_eq!(overall(1, 4.0, None), Some(0.25));
        assert_eq!(overall(1, 4.0, Some(0.5)), Some(0.375));
        assert_eq!(overall(0, 4.0, None), None);
    }
    use serde_json::json;

    fn action(v: Value) -> Action {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn references() {
        assert_eq!(parse_reference("arcade.box/box:arcade.image.convert#webp"), Some(("arcade.box", "box:arcade.image.convert", Some("webp"))));
        assert_eq!(parse_reference("arcade.look/look.preview"), Some(("arcade.look", "look.preview", None)));
        assert_eq!(parse_reference("nonsense"), None);
    }

    #[test]
    fn several_files_go_as_one_array_only_when_the_action_takes_arrays() {
        let dir = std::env::temp_dir().join(format!("arcade-helper-jobs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = ["a.png", "b.svg"].iter().map(|n| dir.join(n)).collect();
        for p in &paths {
            std::fs::write(p, b"x").unwrap();
        }
        let batch = action(json!({"id": "x", "title": "X", "accepts": ["file/image", "file/image[]"]}));
        let runs = runs_for(&batch, &paths);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0][0].kind, "file/image[]");
        assert_eq!(runs[0][0].paths.len(), 2);
        let single = action(json!({"id": "x", "title": "X", "accepts": ["file/image"]}));
        let runs = runs_for(&single, &paths);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1][0].kind, "file/svg");
        assert_eq!(runs[1][0].size, Some(1));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn requests_carry_form_defaults_and_the_preset() {
        let a = action(json!({"id": "box:img", "version": 2, "title": "X", "accepts": ["file/image"],
            "options": {"version": 1, "controls": [{"key": "q", "label": "Quality", "type": "number", "default": 80, "minimum": 1, "maximum": 100}]}}));
        let r = request("arcade.box/box:img#small", &a, vec![], &Map::new(), Origin::FileMenu).unwrap();
        assert_eq!((r.action.as_str(), r.preset.as_deref(), r.version), ("box:img", Some("small"), Some(2)));
        assert_eq!(r.options, json!({"q": 80}));
        assert_eq!(r.context.source, "file-manager");
        let bad = json!({"q": 500});
        assert!(request("arcade.box/box:img", &a, vec![], bad.as_object().unwrap(), Origin::FileMenu).is_err());
    }

    #[test]
    fn targets() {
        let plain = action(json!({"id": "x", "title": "X"}));
        let input = Path::new("/home/u/Pictures/a.png");
        assert_eq!(target(&plain, &Prefs::default(), Origin::FileMenu, Some(input)), Target::Folder("/home/u/Pictures".into()));
        assert_eq!(target(&plain, &Prefs::default(), Origin::Browser, Some(input)), Target::Folder(downloads_dir()));
        let ask = Prefs { ask_where: true, ..Prefs::default() };
        assert_eq!(target(&plain, &ask, Origin::FileMenu, Some(input)), Target::Ask);
        let fixed = Prefs { output_folder: "/data/out".into(), ..Prefs::default() };
        assert_eq!(target(&plain, &fixed, Origin::FileMenu, Some(input)), Target::Folder("/data/out".into()));
        let none = action(json!({"id": "x", "title": "X", "output": "none"}));
        assert_eq!(target(&none, &ask, Origin::FileMenu, Some(input)), Target::Keep);
    }

    #[test]
    fn placing_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("arcade-helper-place-{}", std::process::id()));
        let app = dir.join("app-output");
        let out = dir.join("out");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(app.join("photo.webp"), b"new").unwrap();
        std::fs::write(out.join("photo.webp"), b"mine").unwrap();
        let results = vec![Content::file(&app.join("photo.webp")), Content::plain("hello")];
        let placed = place(&results, Some(&out)).unwrap();
        assert_eq!(placed.files, [out.join("photo (1).webp")]);
        assert_eq!(std::fs::read(out.join("photo.webp")).unwrap(), b"mine");
        assert_eq!(placed.texts, ["hello"]);
        assert!(app.join("photo.webp").exists(), "copied, not moved");
        std::fs::remove_dir_all(dir).ok();
    }
}
