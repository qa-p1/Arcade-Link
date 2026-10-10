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
        let value = entry.trim().trim_matches('"').trim_end_matches(['\\', '/']);
        let expanded = value.replace("%LOCALAPPDATA%", &std::env::var("LOCALAPPDATA").unwrap_or_default());
        expanded.eq_ignore_ascii_case(bin.to_string_lossy().trim_end_matches(['\\', '/']))
    }
    pub fn add_path(environment: &Environment, bin: &Path) -> io::Result<()> {
        let key = key(environment)?;
        let (path, kind) = read(&key)?;
        if path.split(';').any(|p| same(p, bin)) {
            return Ok(());
        }
        let new = if path.is_empty() { bin.display().to_string() } else { format!("{};{}", path.trim_end_matches(';'), bin.display()) };
        write(&key, &new, kind, environment.windows_environment_key == "Environment")
    }
    pub fn remove_path_if_last(environment: &Environment, bin: &Path) -> io::Result<()> {
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
        if !crate::shortcuts::identifier(cli_name) {
            return Err(io::Error::other("invalid shim name"));
        }
        let bin = environment.bin.clone();
        super::super::remove_file(&bin.join(format!("{cli_name}.cmd")))?;
        remove_path_if_last(environment, &bin)
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
        if !crate::shortcuts::identifier(cli_name) || !executable.is_absolute() {
            return Err(io::Error::other("invalid CLI name or target"));
        }
        let target = environment.bin.join(cli_name);
        super::super::symlink(&target, executable)?;
        Ok(target)
    }
}
