//! The subs cell as three real replicas on loopback (repo:ce/docs/LAUNCH-SLICE-2026-11-21.md 7.1): a caller sees only
//! its own tasks, a caller without `subs:usage` gets bands, a `Limited` account moves the task with both reasons, and
//! `Off` makes zero window starts. Calls go through the cell's own `call` (a chain from the owner, SEVEN_RIGHTS).
use serde_json::{json, Value};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_subs-cell");

const REGISTRY: &str = r#"{"accounts":[
 {"name":"a","device":"dev-1","use":{"vendor":"claude","window_ends":null,"week_pct":20,"limited_until":null,"ok":true},"sim":true},
 {"name":"b","device":"dev-2","use":{"vendor":"claude","window_ends":null,"week_pct":50,"limited_until":null,"ok":true},"sim":true}]}"#;

struct Cluster {
    kids: Vec<Child>,
    lock: std::path::PathBuf,
    seed: String,
    dir: std::path::PathBuf,
}

impl Drop for Cluster {
    fn drop(&mut self) {
        for k in &mut self.kids {
            let _ = k.kill();
            let _ = k.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn cluster(tag: &str, env: &[(&str, &str)]) -> Cluster {
    let dir = std::env::temp_dir().join(format!("subs-cell-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let key: Value = serde_json::from_slice(&Command::new(BIN).arg("keygen").output().unwrap().stdout).unwrap();
    let ports: Vec<u16> = (0..3).map(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()).collect();
    let mut lock = format!("[cell]\nname = \"subs\"\nroot = \"{}\"\n", key["public"].as_str().unwrap());
    for (i, p) in ports.iter().enumerate() {
        lock.push_str(&format!("[[replica]]\nnode = \"r{i}\"\naddr = \"127.0.0.1:{p}\"\n"));
    }
    let lp = dir.join("cell.lock");
    std::fs::write(&lp, lock).unwrap();
    let kids = (0..3)
        .map(|i| {
            let mut c = Command::new(BIN);
            c.args(["serve", "--me", &i.to_string(), "--dir"]).arg(dir.join(format!("d{i}"))).env("SEVEN_LOCK", &lp).env("SEVEN_AI", "local").env("SUBS_REGISTRY", REGISTRY).stderr(Stdio::null());
            for (k, v) in env {
                c.env(k, v);
            }
            c.spawn().unwrap()
        })
        .collect();
    std::thread::sleep(Duration::from_millis(2500));
    Cluster { kids, lock: lp, seed: key["seed"].as_str().unwrap().to_string(), dir }
}

/// `call` as an agent (its own fixed key) holding `rights`; returns the reply JSON (ok or err).
fn call(c: &Cluster, agent: u8, rights: &str, args: &[&str]) -> Value {
    let o = Command::new(BIN)
        .arg("call")
        .args(args)
        .env("SEVEN_LOCK", &c.lock)
        .env("SEVEN_OWNER_SEED", &c.seed)
        .env("SEVEN_AGENT_SEED", format!("{agent:02x}").repeat(32))
        .env("SEVEN_RIGHTS", rights)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    match serde_json::from_str::<Value>(&out) {
        Ok(v) => v["reply"].clone(),
        Err(_) => {
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            json!({ "err": serde_json::from_str::<Value>(&err).unwrap_or(json!(err)) })
        }
    }
}

#[test]
fn describe_and_skill() {
    let d: Value = serde_json::from_slice(&Command::new(BIN).arg("describe").output().unwrap().stdout).unwrap();
    let grant = |op: &str| d["ops"].as_array().unwrap().iter().find(|o| o["op"] == op).unwrap()["grant"].clone();
    assert_eq!((grant("submit"), grant("set_plan"), grant("windows"), grant("tick")), (json!("Passkey"), json!("Passkey"), json!("None"), json!("None")));
    let skill = String::from_utf8(Command::new(BIN).arg("skill").output().unwrap().stdout).unwrap();
    assert_eq!(include_str!("../SKILL.md"), skill, "cells/subs/SKILL.md is stale: run `subs-cell skill > cells/subs/SKILL.md`");
}

#[test]
fn bands_without_usage_and_own_tasks_only() {
    let c = cluster("view", &[]);
    let w = call(&c, 1, "subs:read", &["windows", "null", "--key", "w1"]);
    let rows = w["out"].as_array().unwrap_or_else(|| panic!("{w:#}"));
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["week_pct"].is_null() && r["device"].is_null()), "{w:#}");
    assert_eq!((rows[0]["band"].clone(), rows[1]["band"].clone()), (json!("Low"), json!("Mid")));
    let w = call(&c, 1, "subs:read,subs:usage", &["windows", "null", "--key", "w2"]);
    assert_eq!(w["out"][0]["week_pct"], 20, "{w:#}");

    // submit without the owner's tap: NeedsGrant; with it: picked a, the reason stored
    let t = r#"{"text":"fix the flaky test"}"#;
    let e = call(&c, 1, "subs:*", &["submit", t, "--key", "t1"]);
    assert!(e.to_string().contains("NeedsGrant"), "{e:#}");
    let ok = call(&c, 1, "subs:*", &["submit", t, "--key", "t2", "--grant"]);
    assert_eq!((ok["out"]["account"].clone(), ok["out"]["state"].clone()), (json!("a"), json!("Done")), "{ok:#}");
    assert!(ok["out"]["why"].as_str().unwrap().contains("a: 20% of the week used"), "{ok:#}");
    call(&c, 2, "subs:*", &["submit", r#"{"text":"another agent's task"}"#, "--key", "t3", "--grant"]);

    // agent 1 sees its own task, agent 2 its own, a subs:all holder both
    std::thread::sleep(Duration::from_millis(300));
    let mine = |agent: u8, rights: &str| -> Vec<String> {
        let r = call(&c, agent, rights, &["tasks", "{}", "--key", &format!("q{agent}{}", rights.len())]);
        r["out"].as_array().unwrap_or_else(|| panic!("{r:#}")).iter().map(|t| t["text"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(mine(1, "subs:read"), vec!["fix the flaky test"]);
    assert_eq!(mine(2, "subs:read"), vec!["another agent's task"]);
    assert_eq!(mine(3, "subs:read,subs:all").len(), 2);
}

#[test]
fn a_limited_account_moves_the_task_with_both_reasons() {
    let c = cluster("limit", &[("SUBS_SIM_LIMITED", "a")]);
    let ok = call(&c, 1, "subs:*", &["submit", r#"{"text":"t"}"#, "--key", "L1", "--grant"]);
    let why = ok["out"]["why"].as_str().unwrap_or_else(|| panic!("{ok:#}"));
    assert_eq!(ok["out"]["account"], "b", "{ok:#}");
    assert!(why.contains("a answered Limited") && why.contains("picked again: b"), "{why}");
}

#[test]
fn off_makes_zero_starts_and_queued_one() {
    let dir = std::env::temp_dir().join(format!("subs-starts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let starts = dir.join("starts.log");
    let cmd = format!("echo $LABEL >> {}; echo '{{\"started\":true}}'", starts.display());
    // no account serves gemini: the task waits, so Queued has a reason to start
    let c = cluster("sched", &[("SUBS_WINDOW_CMD", &cmd)]);
    let plan = |s: Value, k: &str| call(&c, 9, "subs:*", &["set_plan", &json!({ "account": "a", "schedule": s, "ceiling_pct": 85, "starts_per_day": 1 }).to_string(), "--key", k, "--grant"]);
    assert_eq!(plan(json!("Off"), "p1")["out"]["schedule"], "Off");
    call(&c, 1, "subs:*", &["submit", r#"{"text":"waits","vendor":"gemini"}"#, "--key", "w1", "--grant"]);
    let t = call(&c, 9, "subs:*", &["tick", "null", "--key", "k1"]);
    assert_eq!(t["out"], json!([]), "{t:#}");
    assert!(!starts.exists(), "Off asked for nothing");
    // Queued on the sim account with a waiting task: one start, counted; the same tick request again is replayed
    plan(json!("Queued"), "p2");
    let t = call(&c, 9, "subs:*", &["tick", "null", "--key", "k2"]);
    let again = call(&c, 9, "subs:*", &["tick", "null", "--key", "k2"]);
    assert_eq!(again["replayed"], true);
    assert_eq!(std::fs::read_to_string(&starts).unwrap().lines().count(), 1, "{t:#}");
    let _ = std::fs::remove_dir_all(dir);
}
