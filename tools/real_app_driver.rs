//! Adapter to the actual sibling Tools core. No app implementation is copied.
//! Built into an ignored directory with sibling path dependencies by e2e.py.
use arcade_shortcuts::{build_index, hyprland, lua, BuildInputs};
use arcade_tools_core::{
    catalog::BundleApps,
    diagnostics_archive,
    manager::{LaunchMode, Manager, Request},
    paths::Paths,
    portable_backup,
    self_update::StagedSelfUpdate,
    source::Source,
};
use serde_json::{json, Value};
use std::{io, path::PathBuf, sync::atomic::AtomicBool, time::Instant};

fn run() -> Result<Value, Box<dyn std::error::Error>> {
    if std::env::var("ARCADE_E2E_INNER").as_deref() != Ok("1") {
        return Err("run this adapter through tools/e2e.py's isolated session".into());
    }
    let input: Value = serde_json::from_reader(io::stdin())?;
    let paths = Paths::discover()?;
    let source = Source::test_server(input["source"].as_str().unwrap_or("http://127.0.0.1:1/"))?;
    let manager = Manager::new(paths, source);
    let cancel = AtomicBool::new(false);
    let mut progress = |value| eprintln!("{}", serde_json::to_string(&value).unwrap());
    Ok(match input["command"].as_str().ok_or("missing command")? {
        "catalog" => json!(manager.refresh_catalog()?),
        "list" => {
            let mut apps = manager.list(true)?;
            if input["includeTools"].as_bool().unwrap_or(false) {
                if let Some(tools) = manager.tools_view()? {
                    apps.push(tools);
                }
            }
            json!(apps)
        }
        "updates" => json!(manager.check_updates()?),
        "operate" => json!({"message": manager.operate(&serde_json::from_value::<Request>(input["request"].clone())?, &cancel, &mut progress)?}),
        "bundle" => {
            let catalog = manager.refresh_catalog()?;
            let bundle = catalog.bundles.iter().find(|b| Some(b.id.as_str()) == input["bundle"].as_str()).ok_or("unknown bundle")?;
            let ids = match &bundle.apps {
                BundleApps::All(_) => catalog.apps.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
                BundleApps::Ids(ids) => ids.clone(),
            };
            let mut results = Vec::new();
            for id in ids {
                let request = serde_json::from_value::<Request>(json!({"id": id, "operation": "install", "mode": "background"}))?;
                results.push(manager.operate(&request, &cancel, &mut progress)?);
            }
            json!(results)
        }
        "diagnostics" => {
            json!(diagnostics_archive::create_archive(&manager.paths, &manager.list(true)?, &PathBuf::from(input["path"].as_str().ok_or("missing path")?))?)
        }
        "backup" => json!(portable_backup::create_archive(&manager, &PathBuf::from(input["path"].as_str().ok_or("missing path")?))?),
        "restore" => {
            let preview = portable_backup::preview_restore(&manager, &PathBuf::from(input["path"].as_str().ok_or("missing path")?))?;
            // The runner explicitly requests this isolated restore after inspecting
            // the ZIP. No interactive app confirmation hook is overridden.
            let restored = portable_backup::apply_restore(&manager, &preview.plan_id, &cancel, &mut progress, &mut |enabled| manager.set_link(enabled))?;
            json!({"preview": preview, "restored": restored})
        }
        "self-stage" => json!(manager.prepare_self_update(LaunchMode::Background, &cancel, &mut progress, &mut |_: &StagedSelfUpdate| Ok(()))?),
        "self-finish" => json!({"message": manager.finish_self_update(&serde_json::from_value(input["plan"].clone())?, &cancel, &mut progress)?}),
        "index" => {
            let mut inputs = BuildInputs {
                registry_dir: Some(manager.paths.link.registry),
                catalog_dir: input["catalog"].as_str().map(PathBuf::from),
                arcade_docs: input["docs"]
                    .as_array()
                    .ok_or("missing docs")?
                    .iter()
                    .map(|v| v.as_str().map(PathBuf::from).ok_or("invalid docs path"))
                    .collect::<Result<_, _>>()?,
                ..Default::default()
            };
            let began = Instant::now();
            let mut runtime_binds = Vec::new();
            if let Some(path) = input["hyprland"].as_str() {
                let path = PathBuf::from(path);
                let config = lua::evaluate(&path, &lua::SandboxOptions::new(path.parent().ok_or("missing parent")?));
                let runtime = input["runtime"].as_str().map(std::fs::read_to_string).transpose()?;
                let runtime = runtime.as_deref().map(arcade_shortcuts::runtime::parse_json).transpose()?;
                runtime_binds = runtime.clone().unwrap_or_default();
                inputs.hyprland = Some(hyprland::merge(arcade_shortcuts::ConfigKind::Lua, config, runtime.as_deref(), &Default::default()));
            }
            let index = build_index(&inputs);
            let elapsed = began.elapsed().as_secs_f64() * 1000.0;
            if let Some(path) = input["output"].as_str() {
                index.write_index(&PathBuf::from(path))?;
            }
            json!({"index": index, "elapsedMs": elapsed, "runtimeBinds": runtime_binds})
        }
        _ => return Err("unknown command".into()),
    })
}
fn main() {
    match run() {
        Ok(result) => println!("{result}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
