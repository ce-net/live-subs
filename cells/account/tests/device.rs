//! The account cell is `home = Device`: one replica, on the device cell.lock names. A run needs the owner's grant
//! for that exact task.
use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_account");

#[test]
fn the_account_cell_is_ten_lines() {
    assert_eq!(include_str!("../src/main.rs").lines().count(), 10);
}

#[test]
fn describe_says_one_replica_on_a_device_and_run_needs_a_passkey() {
    let d: Value = serde_json::from_slice(&Command::new(BIN).arg("describe").output().unwrap().stdout).unwrap();
    assert_eq!((d["home"].as_str(), d["replicas"].as_u64()), (Some("Device"), Some(1)));
    let run = d["ops"].as_array().unwrap().iter().find(|o| o["op"] == "run").unwrap().clone();
    assert_eq!((run["cap"].as_str(), run["grant"].as_str()), (Some("account.run"), Some("Passkey")));
}

fn lock(dir: &std::path::Path, root: &str, replicas: &[(&str, u16)]) -> std::path::PathBuf {
    let mut s = format!("[cell]\nname = \"account\"\nroot = \"{root}\"\n");
    for (node, port) in replicas {
        s.push_str(&format!("[[replica]]\nnode = \"{node}\"\naddr = \"127.0.0.1:{port}\"\n"));
    }
    let p = dir.join(format!("cell-{}.lock", replicas.len() * 10 + replicas[0].0.len()));
    std::fs::write(&p, s).unwrap();
    p
}

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[test]
fn serve_refuses_another_device_and_a_second_replica() {
    let dir = std::env::temp_dir().join(format!("account-dev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = "ab".repeat(32);
    let other = lock(&dir, &root, &[("some-other-device", port())]);
    let o = Command::new(BIN).arg("serve").env("SEVEN_LOCK", &other).env_remove("SEVEN_DEVICE").output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("belongs to some-other-device"), "{}", String::from_utf8_lossy(&o.stderr));
    let two = lock(&dir, &root, &[("local", port()), ("local", port())]);
    let o = Command::new(BIN).arg("serve").env("SEVEN_LOCK", &two).output().unwrap();
    assert!(String::from_utf8_lossy(&o.stderr).contains("exactly one replica"), "{}", String::from_utf8_lossy(&o.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn usage_works_and_run_needs_the_grant_for_this_task() {
    let dir = std::env::temp_dir().join(format!("account-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let key: Value = serde_json::from_slice(&Command::new(BIN).arg("keygen").output().unwrap().stdout).unwrap();
    let l = lock(&dir, key["public"].as_str().unwrap(), &[("local", port())]);
    let sim = r#"{"vendor":"claude","window_ends":null,"week_pct":30,"limited_until":null,"ok":true}"#;
    let mut kid = Command::new(BIN)
        .args(["serve", "--dir"])
        .arg(dir.join("d"))
        .env("SEVEN_LOCK", &l)
        .env("SEVEN_AI", "local")
        .env("ACCOUNT_SIM_USE", sim)
        .env("ACCOUNT_SANDBOX_CMD", "printf 'ran: %s\\n' \"$TASK\"")
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let call = |args: &[&str]| {
        let o = Command::new(BIN).arg("call").args(args).env("SEVEN_LOCK", &l).env("SEVEN_OWNER_SEED", key["seed"].as_str().unwrap()).output().unwrap();
        (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr))
    };
    let (ok, out) = call(&["usage", "null"]);
    assert!(ok && out.contains("\"week_pct\": 30"), "{out}");
    let task = r#"{"text":"say hi; rm -rf /"}"#;
    let (ok, out) = call(&["run", task]);
    assert!(!ok && out.contains("passkey grant"), "{out}");
    let (ok, out) = call(&["run", task, "--grant"]);
    // the task text reaches the sandbox as data, not as shell
    assert!(ok && out.contains("ran: say hi; rm -rf /"), "{out}");
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

fn serve_with(dir: &std::path::Path, l: &std::path::Path, env: &[(&str, &str)]) -> std::process::Child {
    let sim = r#"{"vendor":"claude","window_ends":null,"week_pct":30,"limited_until":null,"ok":true}"#;
    let mut c = Command::new(BIN);
    c.args(["serve", "--dir"]).arg(dir.join("d")).env("SEVEN_LOCK", l).env("SEVEN_AI", "local").env("ACCOUNT_SIM_USE", sim).stderr(Stdio::null());
    for (k, v) in env {
        c.env(k, v);
    }
    let kid = c.spawn().unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    kid
}

fn call_as(l: &std::path::Path, seed: &str, args: &[&str]) -> (bool, String) {
    let o = Command::new(BIN).arg("call").args(args).env("SEVEN_LOCK", l).env("SEVEN_OWNER_SEED", seed).output().unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr))
}

/// repo:ce/docs/LAUNCH-SLICE-2026-11-21.md 7.2: a device row that lists no Claude refuses Claude; a refused lease is no
/// run; window_start is refused at the ceiling and once the day's count is used.
#[test]
fn not_on_this_device_lease_refused_and_window_limits() {
    let dir = std::env::temp_dir().join(format!("account-72-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let key: Value = serde_json::from_slice(&Command::new(BIN).arg("keygen").output().unwrap().stdout).unwrap();
    let (root, seed) = (key["public"].as_str().unwrap().to_string(), key["seed"].as_str().unwrap().to_string());

    // a Mac-like row: vendors = ["gemini"]
    let mac = dir.join("mac.lock");
    std::fs::write(&mac, format!("[cell]\nname = \"account\"\nroot = \"{root}\"\n[[replica]]\nnode = \"local\"\naddr = \"127.0.0.1:{}\"\nvendors = [\"gemini\"]\n", port())).unwrap();
    let mut kid = serve_with(&dir.join("m"), &mac, &[]);
    let (ok, out) = call_as(&mac, &seed, &["usage", "null"]);
    assert!(!ok && out.contains("NotOnThisDevice"), "{out}");
    let _ = kid.kill();
    let _ = kid.wait();

    // a refused lease: the sandbox never runs
    let l = lock(&dir, &root, &[("local", port())]);
    let ran = dir.join("ran");
    let mut kid = serve_with(&dir.join("x"), &l, &[("ACCOUNT_LEASE_CMD", "echo '{\"Refused\":\"no lease\"}'; exit 1"), ("ACCOUNT_SANDBOX_CMD", &format!("touch {}", ran.display()))]);
    let (ok, out) = call_as(&l, &seed, &["run", r#"{"text":"hi"}"#, "--grant"]);
    assert!(!ok && out.contains("Lease refused"), "{out}");
    assert!(!ran.exists(), "no lease, no run");
    // window_start: one a day, the second refused; at the ceiling refused
    let (ok, out) = call_as(&l, &seed, &["window_start", "null", "--key", "w1"]);
    assert!(!ok && out.contains("Lease refused"), "the lease gates a start too: {out}");
    let _ = kid.kill();
    let _ = kid.wait();
    let mut kid = serve_with(&dir.join("y"), &l, &[]);
    let (ok, out) = call_as(&l, &seed, &["window_start", "null", "--key", "w2"]);
    assert!(ok && out.contains("\"started\": true"), "{out}");
    let (ok, out) = call_as(&l, &seed, &["window_start", "null", "--key", "w3"]);
    assert!(!ok && out.contains("OverCount"), "{out}");
    let _ = kid.kill();
    let _ = kid.wait();
    let mut kid = serve_with(&dir.join("z"), &l, &[("ACCOUNT_CEILING_PCT", "30")]);
    let (ok, out) = call_as(&l, &seed, &["window_start", "null", "--key", "w4"]);
    assert!(!ok && out.contains("AtCeiling"), "{out}");
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
