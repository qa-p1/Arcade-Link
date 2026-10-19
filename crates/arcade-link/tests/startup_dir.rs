//! The working directory across `childenv::on_startup` and `enter_image_dir`.
//! Its own test binary: both change the process's directory. AppImages are
//! Linux only.
#![cfg(target_os = "linux")]

use arcade_link::childenv;
use std::path::PathBuf;

#[test]
fn a_webview_app_goes_back_into_the_image_after_reading_its_command_line() {
    let root = std::env::temp_dir().join(format!("arcade-startup-dir-{}", std::process::id()));
    let image = root.join(".mount_ArcadeTest/usr");
    let user = root.join("work");
    std::fs::create_dir_all(&image).unwrap();
    std::fs::create_dir_all(&user).unwrap();
    let (image, user): (PathBuf, PathBuf) = (image.canonicalize().unwrap(), user.canonicalize().unwrap());
    // What the AppImage runtime and AppRun leave: $OWD, $APPDIR and $APPDIR/usr.
    std::env::set_var("OWD", &user);
    std::env::set_var("APPDIR", image.parent().unwrap());
    std::env::set_current_dir(&image).unwrap();

    childenv::on_startup();
    assert_eq!(std::env::current_dir().unwrap(), user, "relative paths on the command line are the user's");
    childenv::enter_image_dir();
    assert_eq!(std::env::current_dir().unwrap(), image, "WebKit finds its helpers relative to $APPDIR/usr");

    std::env::set_current_dir(&root).unwrap();
    let _ = std::fs::remove_dir_all(&root);
}
