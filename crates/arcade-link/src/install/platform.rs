//! Small native integration helpers. No shell profiles or quarantine edits.

#[cfg(windows)]
pub mod windows {
    use crate::{install::Environment, paths};
    use std::{fs, io, path::Path};
    use windows_sys::Win32::{Foundation::ERROR_SUCCESS, System::Registry::*, UI::WindowsAndMessaging::*};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn checked(code: u32) -> io::Result<()> {
        if code == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(code as i32))
        }
    }
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn key(environment: &Environment) -> io::Result<Key> {
        let mut key = std::ptr::null_mut();
        unsafe {
            checked(RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wide(&environment.windows_environment_key).as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            ))?;
        }
        Ok(Key(key))
    }
    fn read(key: &Key) -> io::Result<(String, u32)> {
        let mut kind = REG_EXPAND_SZ;
        let mut size = 0;
        let code = unsafe { RegQueryValueExW(key.0, wide("Path").as_ptr(), std::ptr::null(), &mut kind, std::ptr::null_mut(), &mut size) };
        if code == 2 {
            return Ok((String::new(), REG_EXPAND_SZ));
        }
        checked(code)?;
        if ![REG_SZ, REG_EXPAND_SZ].contains(&kind) {
            return Err(io::Error::other("user Path is not a string"));
        }
        let mut buf = vec![0u16; (size as usize / 2) + 1];
        unsafe {
            checked(RegQueryValueExW(key.0, wide("Path").as_ptr(), std::ptr::null(), &mut kind, buf.as_mut_ptr().cast(), &mut size))?;
        }
        let end = buf.iter().position(|v| *v == 0).unwrap_or(buf.len());
        Ok((String::from_utf16_lossy(&buf[..end]), kind))
    }
    fn write(key: &Key, value: &str, kind: u32, broadcast: bool) -> io::Result<()> {
        let value = wide(value);
        unsafe {
            checked(RegSetValueExW(key.0, wide("Path").as_ptr(), 0, kind, value.as_ptr().cast(), (value.len() * 2) as u32))?;
            // Other applications receive the change; never alter machine Path.
            if broadcast {
                SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, wide("Environment").as_ptr() as isize, SMTO_ABORTIFHUNG, 5000, std::ptr::null_mut());
            }
        }
        Ok(())
    }
    fn same(entry: &str, bin: &Path) -> bool {
        use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
        let value = wide(entry.trim().trim_matches('"'));
        let size = unsafe { ExpandEnvironmentStringsW(value.as_ptr(), std::ptr::null_mut(), 0) };
        if size == 0 {
            return false;
        }
        let mut expanded = vec![0u16; size as usize];
        let written = unsafe { ExpandEnvironmentStringsW(value.as_ptr(), expanded.as_mut_ptr(), size) };
        if written == 0 || written > size {
            return false;
        }
        let expanded = String::from_utf16_lossy(&expanded[..written.saturating_sub(1) as usize]);
        expanded.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(bin.to_string_lossy().trim_end_matches(['\\', '/']))
    }
    pub fn add_path(environment: &Environment, bin: &Path) -> io::Result<()> {
        environment.validate()?;
        crate::receipt::native_absolute(bin)?;
        let key = key(environment)?;
        let (path, kind) = read(&key)?;
        if path.split(';').any(|p| same(p, bin)) {
            return Ok(());
        }
        let new = if path.is_empty() { bin.display().to_string() } else { format!("{};{}", path.trim_end_matches(';'), bin.display()) };
        write(&key, &new, kind, environment.windows_environment_key == "Environment")
    }
    pub fn remove_path_if_last(environment: &Environment, bin: &Path) -> io::Result<()> {
        environment.validate()?;
        crate::receipt::native_absolute(bin)?;
        if fs::read_dir(bin).is_ok_and(|d| d.flatten().any(|e| e.path().extension().is_some_and(|s| s.eq_ignore_ascii_case("cmd")))) {
            return Ok(());
        }
        let key = key(environment)?;
        let (path, kind) = read(&key)?;
        let new = path.split(';').filter(|p| !same(p, bin)).collect::<Vec<_>>().join(";");
        if new != path {
            write(&key, &new, kind, environment.windows_environment_key == "Environment")?;
        }
        Ok(())
    }
    pub fn write_shim(environment: &Environment, cli_name: &str, executable: &Path) -> io::Result<std::path::PathBuf> {
        environment.validate()?;
        crate::receipt::native_absolute(executable)?;
        if !crate::shortcuts::identifier(cli_name) || executable.to_string_lossy().contains(['"', '\r', '\n']) {
            return Err(io::Error::other("invalid shim name or executable"));
        }
        let bin = environment.bin.clone();
        let path = bin.join(format!("{cli_name}.cmd"));
        let target = executable.to_string_lossy().replace('%', "%%");
        paths::write_atomic(&path, format!("@echo off\r\n\"{target}\" %*\r\n").as_bytes(), false)?;
        add_path(environment, &bin)?;
        Ok(path)
    }
    pub fn remove_shim(environment: &Environment, cli_name: &str) -> io::Result<()> {
        environment.validate()?;
        if !crate::shortcuts::identifier(cli_name) {
            return Err(io::Error::other("invalid shim name"));
        }
        let bin = environment.bin.clone();
        super::super::remove_file(&bin.join(format!("{cli_name}.cmd")))?;
        remove_path_if_last(environment, &bin)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn isolated_shims_share_one_path_entry_and_remove_it_last() {
            let root = std::env::temp_dir().join(format!("arcade-path-{}", crate::endpoint::new_token().unwrap()));
            let environment = Environment::under(&root);
            assert_ne!(environment.windows_environment_key, "Environment");
            let executable = root.join("sample.exe");
            fs::create_dir_all(&root).unwrap();
            fs::write(&executable, b"fixture").unwrap();
            let key = key(&environment).unwrap();
            write(&key, "C:\\Existing", REG_EXPAND_SZ, false).unwrap();
            let first = write_shim(&environment, "arcade-first", &executable).unwrap();
            write_shim(&environment, "arcade-first", &executable).unwrap();
            write_shim(&environment, "arcade-second", &executable).unwrap();
            let (path, kind) = read(&key).unwrap();
            assert_eq!(kind, REG_EXPAND_SZ);
            assert_eq!(path.split(';').filter(|p| same(p, &environment.bin)).count(), 1);
            assert!(fs::read_to_string(first).unwrap().contains("%*"));
            remove_shim(&environment, "arcade-first").unwrap();
            assert_eq!(read(&key).unwrap().0, path);
            remove_shim(&environment, "arcade-second").unwrap();
            assert_eq!(read(&key).unwrap().0, "C:\\Existing");
            drop(key);
            unsafe {
                checked(RegDeleteTreeW(HKEY_CURRENT_USER, wide(&environment.windows_environment_key).as_ptr())).unwrap();
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}

pub mod macos {
    use crate::install::Environment;
    use std::path::Path;
    #[cfg(unix)]
    use std::{io, path::PathBuf};
    pub fn is_translocated(path: &Path) -> bool {
        path.components().any(|c| c.as_os_str() == "AppTranslocation")
    }
    pub fn bundle_for(executable: &Path) -> Option<std::path::PathBuf> {
        executable.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
    }
    pub fn in_applications(environment: &Environment, bundle: &Path) -> bool {
        bundle.starts_with(environment.home.join("Applications")) || bundle.starts_with("/Applications")
    }
    pub fn cli_symlink_offer(environment: &Environment, cli_name: &str, executable: &Path) -> String {
        let q = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"));
        format!("mkdir -p {} && ln -s {} {}", q(&environment.bin), q(executable), q(&environment.bin.join(cli_name)))
    }
    /// Uses ditto so bundle metadata and quarantine survive the copy. The
    /// caller explicitly chooses a destination; default is ~/Applications.
    #[cfg(target_os = "macos")]
    pub fn move_to_applications(environment: &Environment, bundle: &Path, destination: Option<&Path>) -> io::Result<PathBuf> {
        environment.validate()?;
        crate::receipt::native_absolute(bundle)?;
        if bundle.extension().is_none_or(|s| s != "app") || !bundle.is_dir() {
            return Err(io::Error::other("expected an app bundle"));
        }
        let root = destination.map(PathBuf::from).unwrap_or_else(|| environment.home.join("Applications"));
        if !root.is_absolute() {
            return Err(io::Error::other("Applications path must be absolute"));
        }
        std::fs::create_dir_all(&root)?;
        let target = root.join(bundle.file_name().unwrap());
        if target.exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "bundle already exists"));
        }
        let status = std::process::Command::new("/usr/bin/ditto").arg(bundle).arg(&target).status()?;
        if !status.success() {
            return Err(io::Error::other("ditto failed"));
        }
        Ok(target)
    }
    /// An explicit CLI symlink offer accepted by the user.
    #[cfg(unix)]
    pub fn install_cli(environment: &Environment, cli_name: &str, executable: &Path) -> io::Result<PathBuf> {
        environment.validate()?;
        if !crate::shortcuts::identifier(cli_name) || !executable.is_absolute() {
            return Err(io::Error::other("invalid CLI name or target"));
        }
        let target = environment.bin.join(cli_name);
        super::super::symlink(&target, executable)?;
        Ok(target)
    }

    #[cfg(all(test, target_os = "macos"))]
    mod tests {
        use super::*;
        #[test]
        fn bundle_move_and_cli_use_only_the_explicit_environment() {
            let root = std::env::temp_dir().join(format!("arcade-move-{}", crate::endpoint::new_token().unwrap()));
            let environment = Environment::under(&root);
            let bundle = root.join("download/Find.app");
            let executable = bundle.join("Contents/MacOS/find");
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(&executable, b"fixture").unwrap();
            let installed = move_to_applications(&environment, &bundle, None).unwrap();
            assert!(in_applications(&environment, &installed));
            assert_eq!(std::fs::read(installed.join("Contents/MacOS/find")).unwrap(), b"fixture");
            let cli = install_cli(&environment, "arcade-find", &installed.join("Contents/MacOS/find")).unwrap();
            assert_eq!(std::fs::read_link(cli).unwrap(), installed.join("Contents/MacOS/find"));
            assert!(move_to_applications(&environment, &bundle, None).is_err());
            assert!(bundle.exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
