//! Isolated critical-path probe; worker completion and icon paint are excluded.
use arcade_link::{
    install::{self, Environment, Runtime},
    trayhost::{TrayHostWatcher, WatcherConfig},
    Locations, PeerInfo,
};
use serde_json::json;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("ARCADE_E2E_INNER").as_deref() != Ok("1") {
        return Err("run through tools/e2e.py run --".into());
    }
    let environment = Environment::discover()?;
    let runtime = Runtime::discover()?;
    let mut samples = Vec::new();
    for _ in 0..20 {
        let began = Instant::now();
        let _detection = install::detect(&environment, "arcade.find", &runtime)?;
        let detected = began.elapsed().as_secs_f64() * 1000.0;
        let watcher = TrayHostWatcher::start(
            WatcherConfig {
                locations: Locations::discover(),
                app: PeerInfo { id: "arcade.find".into(), version: "benchmark".into() },
                link_enabled: true,
                has_tray: true,
            },
            |_| {},
        )?;
        let combined = began.elapsed().as_secs_f64() * 1000.0;
        samples.push(json!({"detectMs": detected, "combinedMs": combined}));
        drop(watcher);
        // Let its worker close before the next sample. Outside the measured
        // critical path, and scoped to this finite benchmark process.
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    println!(
        "{}",
        json!({"samples": samples, "budgetMs": 5, "method": "Rust detect() + TrayHostWatcher::start return, development build; excludes worker completion and OS icon painting"})
    );
    Ok(())
}
