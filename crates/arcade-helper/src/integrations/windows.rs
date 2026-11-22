//! Windows Explorer (PLAN §4.5): one cascading "Arcade" menu on files and folders, per user under
//! `HKCU\Software\Classes`. Windows 11 shows it under "Show more options" (the top-level menu needs a signed
//! package; stage 9). Each pinned action is a sub-command filtered to its file types with `AppliesTo`.
//!
//! Explorer starts the command once per selected file; the resident helper gathers those into one job.

use std::io;
use std::path::Path;

use super::{Entry, Plan, RegKey};
use crate::mime;

pub const FILES_ROOT: &str = r"Software\Classes\*\shell\Arcade";
pub const FOLDERS_ROOT: &str = r"Software\Classes\Directory\shell\Arcade";

/// Quotes one argument for a Windows command line (the rules `CommandLineToArgvW` undoes).
pub fn win_quote(arg: &str) -> String {
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

fn command(exe: &Path, entry: &Entry) -> String {
    let mut parts = vec![win_quote(&exe.to_string_lossy())];
    parts.extend(entry.args().iter().map(|a| win_quote(a)));
    parts.push("\"%1\"".into());
    parts.join(" ")
}

/// `System.FileExtension:=.png OR …` for an entry's types, or `None` for every file.
fn applies_to(entry: &Entry) -> Option<String> {
    if entry.is_picker() || mime::takes_any_file(&entry.accepts) {
        return None;
    }
    let exts = mime::extensions(&entry.accepts);
    Some(exts.iter().map(|e| format!("System.FileExtension:=.{e}")).collect::<Vec<_>>().join(" OR "))
}

pub fn plan(exe: &Path, entries: &[Entry]) -> Plan {
    let mut plan = Plan::default();
    if entries.is_empty() {
        return plan;
    }
    let icon = exe.to_string_lossy().to_string();
    for (root, folders) in [(FILES_ROOT, false), (FOLDERS_ROOT, true)] {
        let shown: Vec<(usize, &Entry)> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !folders || e.is_picker() || e.accepts.iter().any(|a| a == "folder/reference"))
            .filter(|(_, e)| folders || e.is_picker() || !mime::extensions(&e.accepts).is_empty())
            .collect();
        if shown.is_empty() {
            continue;
        }
        plan.registry_roots.push(root.to_string());
        plan.registry.push(RegKey {
            path: root.into(),
            values: vec![("MUIVerb".into(), "Arcade".into()), ("SubCommands".into(), String::new()), ("Icon".into(), icon.clone())],
        });
        for (i, entry) in shown {
            let key = format!(r"{root}\shell\{i:02}");
            let mut values = vec![("MUIVerb".into(), entry.title.clone())];
            if !folders {
                if let Some(filter) = applies_to(entry) {
                    values.push(("AppliesTo".into(), filter));
                }
            }
            plan.registry.push(RegKey { path: key.clone(), values });
            plan.registry.push(RegKey { path: format!(r"{key}\command"), values: vec![(String::new(), command(exe, entry))] });
        }
    }
    plan
}

/// Makes the registry match: removes our old trees, then writes the new keys. A no-op off Windows.
pub fn apply(old_roots: &[String], new_roots: &[String], keys: &[RegKey]) -> io::Result<()> {
    #[cfg(windows)]
    {
        for root in old_roots.iter().chain(new_roots) {
            reg::delete_tree(root)?;
        }
        for k in keys {
            reg::set(&k.path, &k.values)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (old_roots, new_roots, keys);
        Ok(())
    }
}

#[cfg(windows)]
mod reg {
    use std::io;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn delete_tree(path: &str) -> io::Result<()> {
        let p = wide(path);
        let rc = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, p.as_ptr()) };
        if rc == ERROR_SUCCESS || rc == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc as i32))
        }
    }

    pub fn set(path: &str, values: &[(String, String)]) -> io::Result<()> {
        let p = wide(path);
        let mut key: HKEY = std::ptr::null_mut();
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                p.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        let mut result = Ok(());
        for (name, value) in values {
            let n = wide(name);
            let v = wide(value);
            let rc = unsafe { RegSetValueExW(key, n.as_ptr(), 0, REG_SZ, v.as_ptr().cast(), (v.len() * 2) as u32) };
            if rc != ERROR_SUCCESS {
                result = Err(io::Error::from_raw_os_error(rc as i32));
                break;
            }
        }
        unsafe { RegCloseKey(key) };
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_command_line_parsing() {
        assert_eq!(win_quote(r"C:\Program Files\Arcade\arcade-helper.exe"), r#""C:\Program Files\Arcade\arcade-helper.exe""#);
        assert_eq!(win_quote(r#"a"b"#), r#""a\"b""#);
        assert_eq!(win_quote(r"ends\"), r#""ends\\""#);
    }

    #[test]
    fn one_cascade_with_filtered_pins() {
        let pin = Entry { reference: "arcade.box/box:pdf.compress".into(), title: "Compress PDF".into(), accepts: vec!["file/pdf".into()] };
        let plan = plan(Path::new(r"C:\Arcade\arcade-helper.exe"), &[Entry::picker(), pin]);
        assert_eq!(plan.registry_roots, [FILES_ROOT, FOLDERS_ROOT]);
        let key = |p: &str| plan.registry.iter().find(|k| k.path == p).unwrap_or_else(|| panic!("{p}"));
        assert!(key(FILES_ROOT).values.contains(&("SubCommands".into(), String::new())));
        assert_eq!(key(&format!(r"{FILES_ROOT}\shell\01")).values[1], ("AppliesTo".into(), "System.FileExtension:=.pdf".into()));
        assert_eq!(key(&format!(r"{FILES_ROOT}\shell\01\command")).values[0].1, r#""C:\Arcade\arcade-helper.exe" "run" "arcade.box/box:pdf.compress" "%1""#);
        assert!(plan.registry.iter().all(|k| !k.path.starts_with(&format!(r"{FOLDERS_ROOT}\shell\01"))), "a PDF action isn't on folders");
    }
}
