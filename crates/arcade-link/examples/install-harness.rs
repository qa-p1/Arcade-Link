//! Headless e2e adapter. Every operation requires an explicit disposable root.
#[cfg(target_os = "linux")]
use arcade_link::{
    install::{self, AppInfo, Environment, Icon, IconSize, InstallOptions, Runtime, UninstallOptions},
    receipt::ManagedBy,
};
#[cfg(target_os = "linux")]
use serde_json::json;
use serde_json::Value;
#[cfg(target_os = "linux")]
use std::{env, path::PathBuf};

#[cfg(target_os = "linux")]
fn run() -> Result<Value, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let root = PathBuf::from(args.get(1).ok_or("needs an absolute root and operation")?);
    let operation = args.get(2).ok_or("needs an operation")?;
    let environment = Environment::under(&root);
    environment.validate()?;
    let app = AppInfo {
        id: "arcade.find".into(),
        name: "Arcade Find".into(),
        filename: "Arcade-Find.AppImage".into(),
        desktop_id: "arcade-find".into(),
        cli_name: "arcade-find".into(),
        autostart_id: "arcade-find".into(),
        background_args: vec!["--background".into()],
        legacy_desktop_ids: vec!["old-find".into()],
    };
    let runtime = Runtime {
        executable: root.join("mount/usr/bin/find"),
        appimage: Some(root.join("download.AppImage")),
        appdir: Some(root.join("mount")),
        dev_build: args.iter().any(|a| a == "--dev"),
    };
    Ok(match operation.as_str() {
        "install" => {
            let icons = [Icon { source: root.join("icon.svg"), size: IconSize::Scalable }];
            json!(install::install(
                &environment,
                InstallOptions {
                    app: &app,
                    runtime: &runtime,
                    version: args.get(3).ok_or("install needs a version")?,
                    channel: "stable",
                    managed_by: ManagedBy::SelfManaged,
                    icons: &icons,
                    start_at_login: true,
                    remove_download: false,
                    refresh_caches: false,
                }
            )?)
        }
        "detect" => {
            let detection = install::detect(&environment, &app.id, &runtime)?;
            json!({"prompt": install::should_prompt(&detection, &[], false, false), "detection": detection})
        }
        "status" => json!(install::integration_status(&environment, &app.id, "fish")?),
        "repair" => json!(install::repair(&environment, &app, args.get(3).map(PathBuf::from).as_deref())?),
        "uninstall" => json!({"removed": install::uninstall(&environment, &app.id, UninstallOptions {remove_data:false,data_folders:&[]})?}),
        _ => return Err("unknown harness operation".into()),
    })
}
#[cfg(not(target_os = "linux"))]
fn run() -> Result<Value, Box<dyn std::error::Error>> {
    Err("the AppImage e2e harness requires Linux; use native receipt/PATH/bundle helpers on this OS".into())
}
fn main() {
    match run() {
        Ok(value) => println!("{}", serde_json::to_string(&value).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
