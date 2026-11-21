//! The two thin layers the account cell stands on.
//!
//! - [`login_usage`]: what this device's login reports. `ACCOUNT_USAGE_CMD` prints either a `Use` or the line
//!   sub.gateway records for the device's window (`{"reported": "...", "limited_until": 0}`). Without it the cell
//!   is in sim mode and reports `ACCOUNT_SIM_USE` (a `Use`, labelled by the caller as `sim`).
//! - [`sandbox_start`]: one run of the task. `ACCOUNT_SANDBOX_CMD` is the deployment's sandbox (ce: agent.box `Start`
//!   with the vendor's own CLI; a laptop: `systemd-run --user --wait -p CPUQuota=..`), and it gets the task text in
//!   the `TASK` environment variable, never spliced into the command line. Without it the run is sim.
//!
//! Neither layer reads or returns a credential. The login stays where the vendor's CLI keeps it, on this device.
pub use ce::serde_json::Value;
pub use ce::Error;
pub use subs::{Task, Use};

fn sh(cmd: &str, env: &[(&str, &str)]) -> Result<(i32, String, u64), Error> {
    let t0 = std::time::Instant::now();
    let mut c = std::process::Command::new("sh");
    c.args(["-c", cmd]);
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().map_err(|e| Error::Backend(e.to_string()))?;
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    Ok((o.status.code().unwrap_or(-1), out, t0.elapsed().as_millis() as u64))
}

pub fn login_usage() -> Result<Use, Error> {
    let vendor = std::env::var("ACCOUNT_VENDOR").unwrap_or_else(|_| "claude".into());
    let Ok(cmd) = std::env::var("ACCOUNT_USAGE_CMD") else {
        let sim = std::env::var("ACCOUNT_SIM_USE").map_err(|_| Error::Backend("no ACCOUNT_USAGE_CMD and no ACCOUNT_SIM_USE".into()))?;
        return serde_json::from_str(&sim).map_err(Error::input);
    };
    let (code, out, _) = sh(&cmd, &[])?;
    if code != 0 {
        return Err(Error::Backend(format!("usage readout exited {code}")));
    }
    let v: Value = serde_json::from_str(out.trim()).map_err(Error::input)?;
    if v.get("week_pct").is_some() {
        return serde_json::from_value(v).map_err(Error::input);
    }
    let reported = v["reported"].as_str().unwrap_or("");
    Ok(Use::from_reported(&vendor, reported, v["limited_until"].as_u64().unwrap_or(0)))
}

pub fn sandbox_start(t: &Task) -> Result<Value, Error> {
    if t.text.trim().is_empty() || t.text.len() > 4000 {
        return Err(Error::Input("a task is 1-4000 bytes of text".into()));
    }
    let Ok(cmd) = std::env::var("ACCOUNT_SANDBOX_CMD") else {
        return Ok(serde_json::json!({ "sim": true, "task": t.text, "exit": 0, "ms": 0 }));
    };
    let (exit, out, ms) = sh(&cmd, &[("TASK", &t.text)])?;
    let tail: String = out.lines().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    Ok(serde_json::json!({ "sim": false, "exit": exit, "ms": ms, "out_tail": tail }))
}
