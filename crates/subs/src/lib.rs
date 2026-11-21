//! `subs`: the owner's subscriptions, one `account` cell per device, and the three things the gateway decides.
//!
//! - [`Registry::add`]: one account per device per vendor. A second one on the same device is refused.
//! - [`pick`]: which account takes the next task, and why. The reason is stored with the task.
//! - [`Schedule`]: `Off` by default. `Queued` starts a window only when a task waits for that account; `Fixed` is
//!   the owner's explicit choice of start times. `Off` makes zero asks.
//!
//! `subs` moves tasks. It never holds or moves a credential and never carries model traffic: there is no
//! API-compatible endpoint here, so there is nothing to share, resell or proxy. The run is the vendor's own CLI,
//! started by the account cell on its own device.
use serde::{Deserialize, Serialize};
use std::fmt;

/// What one account cell reports (`account.usage`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Use {
    pub vendor: String,
    /// unix seconds when the current weekly window resets, if the vendor says
    pub window_ends: Option<u64>,
    /// percent of the weekly allowance used
    pub week_pct: u8,
    /// unix seconds until which the vendor has said "limit reached"
    pub limited_until: Option<u64>,
    /// the login works and the CLI answers
    pub ok: bool,
}

impl Use {
    /// From the line sub.gateway records for a window, e.g.
    /// `seven_day window: 12% used, status allowed, resets at unix 1795683600`, plus its `limited_until` (0 = none).
    pub fn from_reported(vendor: &str, reported: &str, limited_until: u64) -> Use {
        let pct = reported.split('%').next().and_then(|a| a.rsplit(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse::<u8>().ok());
        let ends = reported.rsplit("unix ").next().and_then(|n| n.trim().parse::<u64>().ok()).filter(|_| reported.contains("unix "));
        let limited = reported.contains("rejected") || reported.contains("limit reached");
        Use {
            vendor: vendor.into(),
            window_ends: ends,
            week_pct: pct.unwrap_or(0).min(100),
            limited_until: if limited_until > 0 { Some(limited_until) } else if limited { ends } else { None },
            ok: pct.is_some(),
        }
    }
}

/// One account cell as the gateway sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// the owner's label (`a`, `b`); never the login itself
    pub name: String,
    pub device: String,
    #[serde(rename = "use")]
    pub usage: Use,
    /// `sim` accounts stand in for a device the mesh does not have
    #[serde(default)]
    pub sim: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refused {
    /// a device already has an account for this vendor
    DeviceTaken { device: String, vendor: String, by: String },
    NameTaken(String),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::DeviceTaken { device, vendor, by } => write!(f, "{device} already has a {vendor} account ({by}): one account per device"),
            Refused::NameTaken(n) => write!(f, "an account named {n} exists"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    pub accounts: Vec<Account>,
}

impl Registry {
    pub fn add(&mut self, a: Account) -> Result<(), Refused> {
        if let Some(b) = self.accounts.iter().find(|b| b.device == a.device && b.usage.vendor == a.usage.vendor) {
            return Err(Refused::DeviceTaken { device: a.device, vendor: a.usage.vendor, by: b.name.clone() });
        }
        if self.accounts.iter().any(|b| b.name == a.name) {
            return Err(Refused::NameTaken(a.name));
        }
        self.accounts.push(a);
        Ok(())
    }
    pub fn set_use(&mut self, name: &str, u: Use) {
        if let Some(a) = self.accounts.iter_mut().find(|a| a.name == name) {
            a.usage = u;
        }
    }
    /// `subs.Windows`: every account's window, weekly percent and limit, in one view.
    pub fn windows(&self, now: u64) -> Vec<Window> {
        self.accounts
            .iter()
            .map(|a| Window {
                account: a.name.clone(),
                device: a.device.clone(),
                vendor: a.usage.vendor.clone(),
                week_pct: a.usage.week_pct,
                resets_in_s: a.usage.window_ends.map(|e| e.saturating_sub(now)),
                limited: a.usage.limited_until.map(|u| u > now).unwrap_or(false),
                ok: a.usage.ok,
                sim: a.sim,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub account: String,
    pub device: String,
    pub vendor: String,
    pub week_pct: u8,
    pub resets_in_s: Option<u64>,
    pub limited: bool,
    pub ok: bool,
    pub sim: bool,
}

/// The owner's rule for picking. `ceiling` is the weekly percent above which an account is left alone (default 85).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub ceiling: u8,
}

impl Default for Policy {
    fn default() -> Policy {
        Policy { ceiling: 85 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picked {
    pub account: String,
    pub device: String,
    /// stored with the task: why this account, and why not each other one
    pub why: String,
}

/// `subs.Pick`. In order: the account is `ok`; it is not limited now; its weekly percent is under the ceiling; and
/// of those, the most weekly headroom wins (ties: the earliest reset, then the name). `Err` says why none can.
pub fn pick(r: &Registry, vendor: &str, p: Policy, now: u64) -> Result<Picked, String> {
    let mut skipped = Vec::new();
    let mut ok: Vec<&Account> = Vec::new();
    for a in r.accounts.iter().filter(|a| a.usage.vendor == vendor) {
        let u = &a.usage;
        if !u.ok {
            skipped.push(format!("{}: not ok", a.name));
        } else if let Some(t) = u.limited_until.filter(|&t| t > now) {
            skipped.push(format!("{}: limited for {} more min", a.name, (t - now).div_ceil(60)));
        } else if u.week_pct >= p.ceiling {
            skipped.push(format!("{}: {}% of the week used, ceiling {}%", a.name, u.week_pct, p.ceiling));
        } else {
            ok.push(a);
        }
    }
    ok.sort_by_key(|a| (a.usage.week_pct, a.usage.window_ends.unwrap_or(u64::MAX), a.name.clone()));
    let Some(best) = ok.first() else {
        return Err(if skipped.is_empty() { format!("no {vendor} account") } else { format!("no account can take it ({})", skipped.join("; ")) });
    };
    let mut why = format!("{}: {}% of the week used, the most headroom", best.name, best.usage.week_pct);
    for a in ok.iter().skip(1) {
        why.push_str(&format!("; {}: {}%", a.name, a.usage.week_pct));
    }
    for s in skipped {
        why.push_str(&format!("; skipped {s}"));
    }
    Ok(Picked { account: best.name.clone(), device: best.device.clone(), why })
}

/// `subs.SetSchedule`: when windows are started. `Off` is the default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Schedule {
    #[default]
    Off,
    /// start a window only when a task is waiting for this account
    Queued,
    /// the owner's own start times, minutes after midnight UTC
    Fixed(Vec<u16>),
}

/// How many window starts the scheduler asks for now, for one account with `waiting` queued tasks. `Off` asks for
/// none, ever; a task still runs when it is submitted, through `Submit`, which is the owner's own act.
pub fn starts_due(s: &Schedule, waiting: usize, window_open: bool, minute_of_day: u16) -> u32 {
    match s {
        Schedule::Off => 0,
        Schedule::Queued => u32::from(waiting > 0 && !window_open),
        Schedule::Fixed(times) => u32::from(!window_open && times.contains(&minute_of_day)),
    }
}

/// A task as the card submits it, and what `Submit` stores with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submitted {
    pub task: Task,
    pub picked: Picked,
}
