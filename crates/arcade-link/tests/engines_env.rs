//! `engines::download_tesseract` runs curl with the user's environment, not the calling app's AppImage libraries:
//! from Arcade Tools' image the system curl loaded Tools' older libnghttp2 and failed (Tools D37). Its own file: it
//! changes the process environment.
#![cfg(all(target_os = "linux", target_arch = "x86_64", feature = "engines"))]

#[test]
fn the_tesseract_download_runs_curl_without_the_app_image_libraries() {
    let root = std::env::temp_dir().join(format!("arcade-engines-env-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let dump = root.join("curl-env.txt");
    let curl = bin.join("curl");
    std::fs::write(&curl, format!("#!/bin/sh\nenv > '{}'\necho 'fake curl stops here' >&2\nexit 22\n", dump.display())).unwrap();
    std::fs::set_permissions(&curl, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::set_var("ARCADE_HOME", root.join("home"));
    std::env::set_var("APPDIR", "/tmp/.mount_ArcadeT00ls");
    std::env::set_var("LD_LIBRARY_PATH", "/tmp/.mount_ArcadeT00ls/usr/lib:/opt/keep/lib");
    let Err(error) = arcade_link::engines::download_tesseract() else { panic!("the fake curl fails the download") };
    assert!(error.contains("fake curl stops here"), "{error}");
    let env = std::fs::read_to_string(&dump).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!env.contains(".mount_ArcadeT00ls"), "{env}");
    assert!(env.contains("LD_LIBRARY_PATH=/opt/keep/lib"), "{env}");
}
