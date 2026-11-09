//! `runtime::cap_malloc_arenas` (Tools D18): the process and the WebKit processes it starts get `MALLOC_ARENA_MAX=2`;
//! programs started through `childenv::sanitize` (other apps, installers) do not inherit it. Its own file: it changes
//! the process environment.
#![cfg(all(target_os = "linux", target_env = "gnu"))]
use std::process::Command;

fn arenas(command: &mut Command) -> String {
    let out = command.arg("-c").arg("echo ${MALLOC_ARENA_MAX:-unset}").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn the_cap_reaches_children_but_not_sanitized_programs() {
    std::env::remove_var("MALLOC_ARENA_MAX");
    arcade_link::runtime::cap_malloc_arenas();
    assert_eq!(std::env::var("MALLOC_ARENA_MAX").as_deref(), Ok("2"));
    assert_eq!(arenas(&mut Command::new("sh")), "2", "a WebKit-like child keeps the cap");
    let mut other_app = Command::new("sh");
    arcade_link::childenv::sanitize(&mut other_app);
    assert_eq!(arenas(&mut other_app), "unset", "an app started through sanitize inherited the cap");
}
