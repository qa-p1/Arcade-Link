//! Linux file managers (PLAN §4.5): "Open With" desktop entries (every file manager), Nautilus and Caja scripts,
//! Dolphin service menus (executable, as Plasma 6 requires), Nemo actions and Thunar custom actions.

use std::io;
use std::path::{Path, PathBuf};

use super::{Entry, File, Home, Plan};
use crate::mime;

/// Quotes one argument for a desktop entry's `Exec` (Desktop Entry spec, "The Exec key").
pub fn exec_quote(arg: &str) -> String {
    let mut out = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Quotes one argument for `/bin/sh` (and Thunar, which splits commands the same way).
pub fn sh_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// `exe args… %F` for desktop-entry style launchers.
fn exec_line(exe: &Path, entry: &Entry) -> String {
    let mut parts = vec![exec_quote(&exe.to_string_lossy())];
    parts.extend(entry.args().iter().map(|a| exec_quote(a)));
    parts.push("%F".into());
    parts.join(" ")
}

/// Same for Thunar, whose `%F` is the quoted list of selected files.
fn sh_line(exe: &Path, entry: &Entry) -> String {
    let mut parts = vec![sh_quote(&exe.to_string_lossy())];
    parts.extend(entry.args().iter().map(|a| sh_quote(a)));
    parts.push("%F".into());
    parts.join(" ")
}

/// A menu-safe file name from a title (`Convert to WebP` → `Convert to WebP`, slashes removed).
fn file_title(title: &str) -> String {
    title.chars().map(|c| if c == '/' || c == '\0' { '-' } else { c }).collect::<String>().trim().to_string()
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// The MIME list for an entry; the picker takes every file and folders.
fn mime_list(entry: &Entry) -> String {
    let mut types: Vec<&str> = mime::mime_types(&entry.accepts);
    if entry.accepts.iter().any(|a| a == "folder/reference" || a == "*") {
        types.push("inode/directory");
    }
    types.iter().map(|t| format!("{t};")).collect()
}

/// The desktop file id for an entry: `dev.arcade.helper.desktop` for the picker, `dev.arcade.helper.<n>` for pins.
fn desktop_id(index: usize, entry: &Entry) -> String {
    if entry.is_picker() {
        "dev.arcade.helper".into()
    } else {
        format!("dev.arcade.helper.pin{index}")
    }
}

/// The shell script Nautilus and Caja run (they pass the selected local paths as arguments).
fn script(exe: &Path, entry: &Entry) -> String {
    let mut parts = vec![sh_quote(&exe.to_string_lossy())];
    parts.extend(entry.args().iter().map(|a| sh_quote(a)));
    format!("#!/bin/sh\n# Written by Arcade (arcade-helper); rewritten when your Arcade apps change.\nexec {} \"$@\"\n", parts.join(" "))
}

/// One Thunar custom action.
#[derive(Debug, Clone, PartialEq)]
pub struct ThunarAction {
    pub unique_id: String,
    pub name: String,
    pub command: String,
    pub patterns: String,
    pub directories: bool,
}

const THUNAR_PREFIX: &str = "arcade-helper-";

pub fn plan(exe: &Path, entries: &[Entry], home: &Home) -> Plan {
    let mut plan = Plan::default();
    let mut thunar = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let id = desktop_id(i, entry);
        let name = one_line(&entry.title);
        let mimes = mime_list(entry);
        // "Open With" (every file manager). NoDisplay keeps it out of app launchers.
        plan.files.push(File {
            path: home.data.join("applications").join(format!("{id}.desktop")),
            contents: format!(
                "[Desktop Entry]\nType=Application\nName={name}\nComment=Arcade\nIcon=dev.arcade.Tools\nExec={}\nMimeType={mimes}\nNoDisplay=true\nTerminal=false\n",
                exec_line(exe, entry)
            ),
            executable: false,
        });
        // Nautilus and Caja: an "Arcade" folder in the Scripts submenu.
        for dir in [home.data.join("nautilus/scripts/Arcade"), home.config.join("caja/scripts/Arcade")] {
            plan.files.push(File { path: dir.join(file_title(&entry.title)), contents: script(exe, entry), executable: true });
        }
        // Dolphin: a service menu per entry, grouped under "Arcade".
        let dolphin_mimes = if entry.is_picker() { "all/allfiles;inode/directory;".to_string() } else { mimes.clone() };
        plan.files.push(File {
            path: home.data.join("kio/servicemenus").join(format!("{id}.desktop")),
            contents: format!(
                "[Desktop Entry]\nType=Service\nServiceTypes=KonqPopupMenu/Plugin\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nMimeType={dolphin_mimes}\nActions=arcade;\nX-KDE-Submenu=Arcade\n\n[Desktop Action arcade]\nName={name}\nIcon=dev.arcade.Tools\nExec={}\n",
                exec_line(exe, entry)
            ),
            executable: true,
        });
        // Nemo.
        let selector = if entry.is_picker() { "Extensions=any;\n".to_string() } else { format!("Mimetypes={mimes}\n") };
        plan.files.push(File {
            path: home.data.join("nemo/actions").join(format!("{id}.nemo_action")),
            contents: format!(
                "[Nemo Action]\nName=Arcade: {name}\nComment=Arcade\nExec={}\nIcon-Name=dev.arcade.Tools\nSelection=notnone\n{selector}",
                exec_line(exe, entry)
            ),
            executable: false,
        });
        // Thunar.
        let patterns = if entry.is_picker() || mime::takes_any_file(&entry.accepts) {
            "*".to_string()
        } else {
            mime::extensions(&entry.accepts).iter().flat_map(|e| [format!("*.{e}"), format!("*.{}", e.to_uppercase())]).collect::<Vec<_>>().join(";")
        };
        thunar.push(ThunarAction {
            unique_id: format!("{THUNAR_PREFIX}{i}"),
            name,
            command: sh_line(exe, entry),
            patterns,
            directories: entry.accepts.iter().any(|a| a == "folder/reference"),
        });
    }
    if !entries.is_empty() {
        plan.thunar = Some((home.config.join("Thunar/uca.xml"), thunar));
    }
    plan
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn thunar_xml(a: &ThunarAction) -> String {
    format!(
        "<action>\n\t<icon>dev.arcade.Tools</icon>\n\t<name>{}</name>\n\t<submenu>Arcade</submenu>\n\t<unique-id>{}</unique-id>\n\t<command>{}</command>\n\t<description>Arcade</description>\n\t<range>*</range>\n\t<patterns>{}</patterns>\n{}\t<audio-files/>\n\t<image-files/>\n\t<other-files/>\n\t<text-files/>\n\t<video-files/>\n</action>\n",
        xml_escape(&a.name),
        xml_escape(&a.unique_id),
        xml_escape(&a.command),
        xml_escape(&a.patterns),
        if a.directories { "\t<directories/>\n" } else { "" }
    )
}

/// `uca.xml` with our actions replaced by `ours`, everything else kept byte for byte. `None` when the user's file
/// isn't one we can safely edit (no `<actions>` element): then it is left alone.
pub fn thunar_merged(existing: Option<&str>, ours: &[ThunarAction]) -> Option<String> {
    let base = existing.unwrap_or("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n");
    let close = base.rfind("</actions>")?;
    let mut kept = String::new();
    let mut rest = &base[..close];
    while let Some(start) = rest.find("<action>") {
        let end = rest[start..].find("</action>")? + start + "</action>".len();
        let block = &rest[start..end];
        kept.push_str(&rest[..start]);
        if !block.contains(&format!("<unique-id>{THUNAR_PREFIX}")) {
            kept.push_str(block);
        } else {
            // Drop the line break after our block too, so repeated merges don't grow the file.
            rest = rest[end..].strip_prefix('\n').unwrap_or(&rest[end..]);
            continue;
        }
        rest = &rest[end..];
    }
    kept.push_str(rest);
    let mut out = kept;
    for a in ours {
        out.push_str(&thunar_xml(a));
    }
    out.push_str(&base[close..]);
    Some(out)
}

/// Rewrites `path` with `ours` merged in; returns whether it changed. A missing file is created only when there is
/// something to add.
pub fn merge_thunar(path: &PathBuf, ours: &[ThunarAction]) -> io::Result<bool> {
    let existing = std::fs::read_to_string(path).ok();
    if existing.is_none() && ours.is_empty() {
        return Ok(false);
    }
    let Some(merged) = thunar_merged(existing.as_deref(), ours) else { return Ok(false) };
    if existing.as_deref() == Some(merged.as_str()) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("xml.arcade-tmp");
    std::fs::write(&tmp, merged)?;
    std::fs::rename(tmp, path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin() -> Entry {
        Entry {
            reference: "arcade.box/box:arcade.image.convert#webp".into(),
            title: "Convert to WebP".into(),
            accepts: vec!["file/image".into(), "file/image[]".into()],
        }
    }

    #[test]
    fn quoting() {
        assert_eq!(exec_quote("/opt/Arcade Tools/arcade-helper"), "\"/opt/Arcade Tools/arcade-helper\"");
        assert_eq!(exec_quote("a$b\"c%d"), "\"a\\$b\\\"c%%d\"");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn every_file_manager_gets_the_picker_and_pins() {
        let home = Home::under(Path::new("/h"));
        let plan = plan(Path::new("/opt/arcade/arcade-helper"), &[Entry::picker(), pin()], &home);
        let file = |p: &str| plan.files.iter().find(|f| f.path == Path::new(p)).unwrap_or_else(|| panic!("{p}"));
        let open_with = file("/h/.local/share/applications/dev.arcade.helper.pin1.desktop");
        assert!(open_with.contents.contains("Exec=\"/opt/arcade/arcade-helper\" \"run\" \"arcade.box/box:arcade.image.convert#webp\" %F"));
        assert!(open_with.contents.contains("image/png;") && open_with.contents.contains("image/svg+xml;"));
        assert!(file("/h/.local/share/applications/dev.arcade.helper.desktop").contents.contains("inode/directory;"));
        let nautilus = file("/h/.local/share/nautilus/scripts/Arcade/Convert to WebP");
        assert!(nautilus.executable && nautilus.contents.contains("exec '/opt/arcade/arcade-helper' 'run' 'arcade.box/box:arcade.image.convert#webp' \"$@\""));
        assert!(file("/h/.config/caja/scripts/Arcade/Arcade…").executable);
        let dolphin = file("/h/.local/share/kio/servicemenus/dev.arcade.helper.desktop");
        assert!(dolphin.executable && dolphin.contents.contains("MimeType=all/allfiles;inode/directory;") && dolphin.contents.contains("X-KDE-Submenu=Arcade"));
        assert!(file("/h/.local/share/nemo/actions/dev.arcade.helper.pin1.nemo_action").contents.contains("Mimetypes=image/png;"));
        let (path, thunar) = plan.thunar.as_ref().unwrap();
        assert_eq!(path, Path::new("/h/.config/Thunar/uca.xml"));
        assert!(thunar[1].patterns.contains("*.png;*.PNG") && !thunar[1].directories && thunar[0].directories);
    }

    #[test]
    fn thunar_merge_keeps_the_users_actions() {
        let user = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n<action>\n\t<name>Open Terminal Here</name>\n\t<unique-id>1234</unique-id>\n</action>\n</actions>\n";
        let ours = plan(Path::new("/x"), &[pin()], &Home::under(Path::new("/h"))).thunar.unwrap().1;
        let once = thunar_merged(Some(user), &ours).unwrap();
        assert!(once.contains("Open Terminal Here") && once.contains("arcade-helper-0") && once.contains("&apos;") == false);
        assert_eq!(thunar_merged(Some(&once), &ours).unwrap(), once, "merging again changes nothing");
        assert_eq!(thunar_merged(Some(&once), &[]).unwrap(), user, "removing ours restores the user's file");
        assert!(thunar_merged(Some("garbage"), &ours).is_none());
        assert!(thunar_merged(None, &ours).unwrap().starts_with("<?xml"));
    }
}
