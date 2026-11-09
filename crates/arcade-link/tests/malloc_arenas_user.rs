//! A user's own `MALLOC_ARENA_MAX` wins over `runtime::cap_malloc_arenas` and reaches every program. Its own file: it
//! changes the process environment.
#![cfg(all(target_os = "linux", target_env = "gnu"))]
use std::process::Command;

#[test]
fn a_user_setting_is_kept_everywhere() {
    std::env::set_var("MALLOC_ARENA_MAX", "4");
    arcade_link::runtime::cap_malloc_arenas();
    assert_eq!(std::env::var("MALLOC_ARENA_MAX").as_deref(), Ok("4"));
    let mut other_app = Command::new("sh");
    arcade_link::childenv::sanitize(&mut other_app);
    let out = other_app.arg("-c").arg("echo $MALLOC_ARENA_MAX").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
}
