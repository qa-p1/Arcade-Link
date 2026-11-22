//! Talking to the resident helper from the short-lived processes (menu entries, the picker, the progress window,
//! the browser bridge). Starts the resident when it isn't running.

use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

use arcade_link::client::{spawn_detached, Client};
use arcade_link::wire::{InvokeRequest, PeerInfo};
use arcade_link::{LinkError, Locations};

use crate::ids;
use crate::resident::{JobView, RunSpec};

/// How long to wait for a resident we just started.
const START_TIMEOUT: Duration = Duration::from_secs(8);

pub struct HelperClient {
    client: Client,
}

fn me(role: &str) -> PeerInfo {
    PeerInfo { id: format!("{}.{role}", ids::HELPER_ID), version: env!("CARGO_PKG_VERSION").into() }
}

impl HelperClient {
    /// Connects to the resident, starting it (`exe --background`) if needed.
    pub fn connect(locations: &Locations, exe: &Path, role: &str) -> Result<HelperClient, LinkError> {
        if let Ok(client) = Client::connect(locations, ids::HELPER_ID, &me(role)) {
            return Ok(HelperClient { client });
        }
        spawn_detached(&exe.to_string_lossy(), &["--background".to_string()])
            .map_err(|e| LinkError::internal(format!("couldn't start the Arcade helper: {e}")))?;
        let start = Instant::now();
        loop {
            match Client::connect(locations, ids::HELPER_ID, &me(role)) {
                Ok(client) => return Ok(HelperClient { client }),
                Err(e) if start.elapsed() > START_TIMEOUT => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    fn call(&mut self, action: &str, options: Value) -> Result<Value, LinkError> {
        let mut request = InvokeRequest::new(action, "helper");
        request.options = options;
        let r = self.client.invoke(&request, &mut |_| {}, None)?;
        Ok(r.data.unwrap_or(Value::Null))
    }

    pub fn run(&mut self, spec: &RunSpec) -> Result<u64, LinkError> {
        let r = self.call("helper.run", serde_json::to_value(spec).map_err(|e| LinkError::internal(e.to_string()))?)?;
        r.get("job").and_then(Value::as_u64).ok_or_else(|| LinkError::internal("no job id"))
    }

    /// Jobs after `since`, waiting up to `wait` for a change. `ui`: called by the progress window.
    pub fn jobs(&mut self, since: u64, wait: Duration, ui: bool) -> Result<(u64, Vec<JobView>), LinkError> {
        let r = self.call("helper.jobs", json!({"since": since, "waitMs": wait.as_millis() as u64, "ui": ui}))?;
        let revision = r.get("revision").and_then(Value::as_u64).unwrap_or(0);
        let jobs = serde_json::from_value(r.get("jobs").cloned().unwrap_or_default()).unwrap_or_default();
        Ok((revision, jobs))
    }

    pub fn cancel(&mut self, job: u64) -> Result<(), LinkError> {
        self.call("helper.cancel", json!({"job": job})).map(|_| ())
    }

    pub fn answer(&mut self, job: u64, folder: Option<&Path>) -> Result<(), LinkError> {
        self.call("helper.answer", json!({"job": job, "folder": folder.map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()})).map(|_| ())
    }

    pub fn dismiss(&mut self, job: Option<u64>) -> Result<(), LinkError> {
        self.call("helper.dismiss", json!({"job": job})).map(|_| ())
    }

    pub fn integrate(&mut self) -> Result<Value, LinkError> {
        self.call("helper.integrate", json!({}))
    }
}
