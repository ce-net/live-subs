//! The two thin layers the account cell stands on.
//!
//! - [`login_usage`]: what this device's login reports. `ACCOUNT_USAGE_CMD` prints either a `Use` or the line
//!   sub.gateway records for the device's window (`{"reported": "...", "limited_until": 0}`). Without it the cell
//!   is in sim mode and reports `ACCOUNT_SIM_USE` (a `Use`, labelled by the caller as `sim`).
//! - [`sandbox_start`]: one run of the task. `ACCOUNT_SANDBOX_CMD` is the deployment's sandbox (ce: agent.box `Start`
//!   with the vendor's own CLI; a laptop: `systemd-run --user --wait -p CPUQuota=..`), and it gets the task text in
//!   the `TASK` environment variable, never spliced into the command line. Without it the run is sim.
//!
//! - [`device_allows`]: the device row in cell.lock may list `vendors`; a vendor it does not list is refused
//!   (`NotOnThisDevice`), so a Mac row that lists no Claude never runs Claude.
//! - [`lease`]: before a run, `ACCOUNT_LEASE_CMD` asks `ai.account Lease` (may this cell run a session on this account
//!   on this node; default deny). A refused or missing answer is no run. Without the command the run is sim.
//! - [`window_start`]: one smallest prompt that opens the vendor's window, refused at the ceiling
//!   (`ACCOUNT_CEILING_PCT`, default 85) or once the day's count (`ACCOUNT_STARTS_PER_DAY`, default 1) is used.
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

pub fn vendor() -> String {
    std::env::var("ACCOUNT_VENDOR").unwrap_or_else(|_| "claude".into())
}

/// The device row's `vendors` (cell.lock), when it states them: a vendor not listed is `NotOnThisDevice`.
pub fn device_allows(vendor: &str) -> Result<(), Error> {
    let Some(p) = ce::lock::Lock::find() else { return Ok(()) };
    let Ok(l) = ce::lock::Lock::load(&p) else { return Ok(()) };
    match l.replicas.first().and_then(|r| r.vendors.clone().map(|v| (r.node.clone(), v))) {
        Some((node, v)) if !v.iter().any(|x| x == vendor) => Err(Error::Denied(format!("NotOnThisDevice: {node} lists {v:?}, not {vendor}"))),
        _ => Ok(()),
    }
}

/// `ai.account Lease` for this cell on this account: `ACCOUNT_LEASE_CMD` exits 0 and prints the lease (`Leased`), or
/// the run does not happen. `Ok(None)`: no command, the run is sim.
pub fn lease() -> Result<Option<Value>, Error> {
    let Ok(cmd) = std::env::var("ACCOUNT_LEASE_CMD") else { return Ok(None) };
    let (code, out, _) = sh(&cmd, &[])?;
    let v: Value = serde_json::from_str(out.trim()).unwrap_or(Value::Null);
    let leased = code == 0 && (v.get("Leased").is_some() || v.pointer("/Ok/Leased").is_some() || v["leased"] == true);
    if !leased {
        return Err(Error::Denied(format!("ai.account Lease refused this cell (exit {code}): no run")));
    }
    Ok(Some(v))
}

pub fn login_usage() -> Result<Use, Error> {
    let vendor = vendor();
    device_allows(&vendor)?;
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
    device_allows(&vendor())?;
    let leased = lease()?;
    let Ok(cmd) = std::env::var("ACCOUNT_SANDBOX_CMD") else {
        return Ok(serde_json::json!({ "sim": true, "task": t.text, "exit": 0, "ms": 0 }));
    };
    let (exit, out, ms) = sh(&cmd, &[("TASK", &t.text)])?;
    let tail: String = out.lines().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    Ok(serde_json::json!({ "sim": false, "leased": leased.is_some(), "exit": exit, "ms": ms, "out_tail": tail }))
}

/// One window start (`account.window`): refused at the ceiling or over the day's count. `starts_today` is what the
/// cell's own records say. The smallest prompt goes through `ACCOUNT_WINDOW_CMD` (the vendor CLI); without it, sim.
pub fn window_start(u: &Use, starts_today: u32) -> Result<Value, Error> {
    device_allows(&vendor())?;
    let ceiling: u8 = std::env::var("ACCOUNT_CEILING_PCT").ok().and_then(|v| v.parse().ok()).unwrap_or(85);
    let per_day: u32 = std::env::var("ACCOUNT_STARTS_PER_DAY").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    if u.week_pct >= ceiling {
        return Err(Error::Denied(format!("AtCeiling: {}% of the week used, ceiling {ceiling}%", u.week_pct)));
    }
    if starts_today >= per_day {
        return Err(Error::Denied(format!("OverCount: {starts_today} of {per_day} window starts used today")));
    }
    lease()?;
    let Ok(cmd) = std::env::var("ACCOUNT_WINDOW_CMD") else { return Ok(serde_json::json!({ "sim": true, "started": true })) };
    let (exit, _, ms) = sh(&cmd, &[])?;
    Ok(serde_json::json!({ "sim": false, "started": exit == 0, "exit": exit, "ms": ms }))
}

/// Today's UTC day number.
pub fn today() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}
