//! The `subs` cell (repo:ce/docs/LAUNCH-SLICE-2026-11-21.md 7.1): one program on the seven-lines runtime, four
//! capabilities. The pick policy is the `subs` crate, unchanged.
//!
//! - `subs.read`: `windows` (every account's window; the weekly percent and the device only for a caller holding
//!   `subs:usage`, a band `low | mid | high` for everyone else, so the public page gets bands) and `tasks` (filtered at
//!   the server: a caller sees its own tasks; a caller holding `subs:all` sees every task).
//! - `subs.submit` (`grant = Passkey`): pick, store the reason, run the task on the picked account, store the result.
//!   An account that answers `Limited` is marked and the task is picked again once, both reasons kept.
//! - `subs.schedule`: `set_plan` (`grant = Passkey`) and `tick`. `Off` is the default and makes zero asks; `Queued`
//!   and `Fixed` ask the account's `window_start`, counted and audited as steps.
//!
//! Where the accounts are and how a run reaches one are the deployment's (cell.lock and the environment):
//! `SUBS_REGISTRY` (a registry JSON, `{"accounts": [...]}`) or `SUBS_REGISTRY_CMD` (prints one: each device's account
//! cell `usage`), `SUBS_RUN_CMD` (runs one task on account `$LABEL` with `$TASK`; in ce the account cell's `run`),
//! `SUBS_WINDOW_CMD` (one window start on `$LABEL`). Without a command a run is sim and says so. Never a login, never
//! model traffic: the run is the vendor's own CLI on the account's own device.
use ce::prelude::*;
use ce::serde_json::{json, Value};
use serde::{Deserialize, Serialize};
use subs::{Policy, Registry, Schedule};

/// A task's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Picked,
    Done,
    Failed,
    /// no account can take it now (the reason says why)
    Waiting,
}

/// One submitted task. `by` is the caller's key; `why` every pick reason, in order.
#[ce::record(cell = subs)]
pub struct Task {
    pub id: String,
    pub by: String,
    pub text: String,
    pub repo: Option<String>,
    pub state: TaskState,
    pub account: Option<String>,
    pub why: String,
    pub result: Value,
}

/// The owner's schedule for one account. `Off` by default; real accounts stay `Off` until the owner turns one on.
#[ce::record(cell = subs)]
pub struct Plan {
    pub account: String,
    pub schedule: Schedule,
    pub ceiling_pct: u8,
    pub starts_per_day: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTask {
    pub text: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub vendor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskQuery {
    #[serde(default)]
    pub state: Option<TaskState>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// How full a week is, for a caller that may not see the percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Band {
    Low,
    Mid,
    High,
}

impl Band {
    pub fn of(pct: u8) -> Band {
        match pct {
            0..=39 => Band::Low,
            40..=79 => Band::Mid,
            _ => Band::High,
        }
    }
}

/// One account's window as a caller may see it. `week_pct` and `device` are `None` without `subs:usage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub label: String,
    pub vendor: String,
    pub ok: bool,
    pub limited: bool,
    pub window_ends: Option<u64>,
    pub band: Band,
    pub week_pct: Option<u8>,
    pub device: Option<String>,
    pub sim: bool,
}

/// One window start the scheduler made (`tick`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Start {
    pub account: String,
    pub why: String,
    pub result: Value,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn sh(cmd: &str, env: &[(&str, &str)]) -> Result<(i32, String)> {
    let mut c = std::process::Command::new("sh");
    c.args(["-c", cmd]);
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().map_err(|e| Error::Backend(e.to_string()))?;
    Ok((o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).to_string()))
}

/// The accounts, each re-added through the one-per-device rule (a registry with two on one device is refused).
pub fn registry() -> Result<Registry> {
    let text = match (std::env::var("SUBS_REGISTRY_CMD"), std::env::var("SUBS_REGISTRY")) {
        (Ok(cmd), _) => {
            let (code, out) = sh(&cmd, &[])?;
            if code != 0 {
                return Err(Error::Backend(format!("registry readout exited {code}")));
            }
            out
        }
        (_, Ok(r)) => r,
        _ => return Err(Error::Backend("no SUBS_REGISTRY_CMD and no SUBS_REGISTRY".into())),
    };
    let r: Registry = serde_json::from_str(text.trim()).map_err(Error::input)?;
    let mut checked = Registry::default();
    for a in r.accounts {
        checked.add(a).map_err(|e| Error::Input(e.to_string()))?;
    }
    Ok(checked)
}

/// `windows`: bands for everyone, the percent and the device for `subs:usage`.
pub fn windows(cx: &Cx) -> Result<Vec<Window>> {
    windows_for(&registry()?, cx.holds("subs:usage"), now())
}

pub fn windows_for(r: &Registry, usage: bool, t: u64) -> Result<Vec<Window>> {
    Ok(r.accounts
        .iter()
        .map(|a| Window {
            label: a.name.clone(),
            vendor: a.usage.vendor.clone(),
            ok: a.usage.ok,
            limited: a.usage.limited_until.is_some_and(|u| u > t),
            window_ends: a.usage.window_ends,
            band: Band::of(a.usage.week_pct),
            week_pct: usage.then_some(a.usage.week_pct),
            device: usage.then(|| a.device.clone()),
            sim: a.sim,
        })
        .collect())
}

/// `tasks`: the caller's own, or every task for `subs:all`; newest last, at most `limit` (default 50).
pub fn tasks_for(cx: &Cx, q: TaskQuery) -> Vec<Task> {
    filter_tasks(cx.twin().all::<Task>().into_iter().map(|(_, t)| t).collect(), cx.caller(), cx.holds("subs:all"), &q)
}

pub fn filter_tasks(all: Vec<Task>, caller: &str, every: bool, q: &TaskQuery) -> Vec<Task> {
    let mut v: Vec<Task> = all.into_iter().filter(|t| every || t.by == caller).filter(|t| q.state.is_none_or(|s| s == t.state)).collect();
    let n = q.limit.unwrap_or(50).min(500) as usize;
    if v.len() > n {
        v.drain(..v.len() - n);
    }
    v
}

/// The ceiling for picking: the lowest `ceiling_pct` of the plans, else the policy default.
fn policy(cx: &Cx) -> Policy {
    let plans: Vec<Plan> = cx.twin().all::<Plan>().into_iter().map(|(_, p)| p).collect();
    plans.iter().map(|p| p.ceiling_pct).filter(|c| *c > 0).min().map(|ceiling| Policy { ceiling }).unwrap_or_default()
}

/// One run of `text` on account `label`: the deployment's `SUBS_RUN_CMD`, or sim. `SUBS_SIM_LIMITED` (labels,
/// comma separated) makes a sim account answer `Limited`, for the forced-limit proof.
pub fn run_on(label: &str, device: &str, text: &str) -> Result<Value> {
    if let Ok(cmd) = std::env::var("SUBS_RUN_CMD") {
        let (code, out) = sh(&cmd, &[("LABEL", label), ("DEVICE", device), ("TASK", text)])?;
        let v: Value = serde_json::from_str(out.trim()).unwrap_or_else(|_| json!({ "exit": code, "out_tail": out.lines().rev().take(20).collect::<Vec<_>>() }));
        return Ok(v);
    }
    let limited = std::env::var("SUBS_SIM_LIMITED").unwrap_or_default().split(',').any(|l| l.trim() == label);
    Ok(if limited { json!({ "sim": true, "limited": true }) } else { json!({ "sim": true, "exit": 0, "account": label }) })
}

fn is_limited(v: &Value) -> bool {
    v["limited"] == json!(true) || v.pointer("/err/Limited").is_some() || v.pointer("/reply/err/Limited").is_some()
}

/// `submit`: pick with its reason, run on the picked account (a step: a new leader replays it, never runs it twice),
/// and on `Limited` mark that account and pick once more.
pub async fn submit(cx: &Cx, t: NewTask) -> Result<Task> {
    if t.text.trim().is_empty() || t.text.len() > 4000 {
        return Err(Error::Input("a task is 1-4000 bytes of text".into()));
    }
    let vendor = t.vendor.clone().unwrap_or_else(|| "claude".into());
    let mut reg = registry()?;
    let p = policy(cx);
    let mut task = Task { id: cx.key().to_string(), by: cx.caller().to_string(), text: t.text.clone(), repo: t.repo.clone(), state: TaskState::Waiting, account: None, why: String::new(), result: Value::Null };
    for round in 0..2 {
        let picked = match subs::pick(&reg, &vendor, p, now()) {
            Ok(x) => x,
            Err(why) => {
                task.why = if task.why.is_empty() { why } else { format!("{}; then {why}", task.why) };
                break;
            }
        };
        task.why = if task.why.is_empty() { picked.why.clone() } else { format!("{}; picked again: {}", task.why, picked.why) };
        task.account = Some(picked.account.clone());
        task.state = TaskState::Picked;
        let (label, device, text) = (picked.account.clone(), picked.device.clone(), t.text.clone());
        let out = cx.step("subs.run", move || async move { run_on(&label, &device, &text) }).await?;
        if is_limited(&out) && round == 0 {
            task.why.push_str(&format!("; {} answered Limited", picked.account));
            if let Some(a) = reg.accounts.iter_mut().find(|a| a.name == picked.account) {
                a.usage.limited_until = Some(now() + 3600);
            }
            task.result = out;
            continue;
        }
        task.state = if is_limited(&out) || out["exit"].as_i64().is_some_and(|e| e != 0) { TaskState::Failed } else { TaskState::Done };
        task.result = out;
        break;
    }
    cx.twin().put(&task).await?;
    Ok(task)
}

/// `tick`: one pass of the scheduler over every plan. `Off` asks for nothing, ever; `Queued` starts a window only
/// while a task waits for that account and its window is closed; a start is refused at the plan's ceiling. Each start
/// is a `subs.window` step (an audit row); the account cell's `window_start` holds the day's count.
pub async fn tick(cx: &Cx) -> Result<Vec<Start>> {
    let plans: Vec<Plan> = cx.twin().all::<Plan>().into_iter().map(|(_, p)| p).collect();
    let mut out = vec![];
    let waiting = tasks_for(cx, TaskQuery { state: Some(TaskState::Waiting), limit: Some(500) });
    let reg = registry().unwrap_or_default();
    let t = now();
    let minute = ((t % 86_400) / 60) as u16;
    for p in plans {
        let open = reg.accounts.iter().find(|a| a.name == p.account).is_some_and(|a| a.usage.window_ends.is_some_and(|e| e > t));
        let mine = waiting.iter().filter(|w| w.account.as_deref().is_none_or(|a| a == p.account)).count();
        let due = subs::starts_due(&p.schedule, mine, open, minute);
        if due == 0 {
            continue;
        }
        let pct = reg.accounts.iter().find(|a| a.name == p.account).map(|a| a.usage.week_pct).unwrap_or(0);
        if p.ceiling_pct > 0 && pct >= p.ceiling_pct {
            out.push(Start { account: p.account.clone(), why: format!("refused: {pct}% of the week used, ceiling {}%", p.ceiling_pct), result: Value::Null });
            continue;
        }
        let label = p.account.clone();
        let r = cx
            .step("subs.window", move || async move {
                match std::env::var("SUBS_WINDOW_CMD") {
                    Ok(cmd) => sh(&cmd, &[("LABEL", &label)]).map(|(code, o)| serde_json::from_str(o.trim()).unwrap_or(json!({ "exit": code }))),
                    Err(_) => Ok(json!({ "sim": true, "started": label })),
                }
            })
            .await?;
        out.push(Start { account: p.account.clone(), why: format!("{:?}: a start is due", p.schedule), result: r });
    }
    Ok(out)
}
