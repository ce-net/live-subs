//! `subs-prove <run dir>`: run 2 (repo:ce/docs/LAUNCH-SLICE-2026-11-21.md 7.5) as one command. The subs cell on 3
//! replicas over two account cells, every process under `systemd-run --user` with CPUQuota=10%, all stopped at the
//! end (also on failure). A fresh scoped test key made for the run signs the one-use grants (never the owner's), the
//! accounts are sim (no real login is read or published), and the second device is sim. Prints the steps as JSON
//! lines and writes them to `<run dir>/run2.jsonl`. The run dir holds locks and keys, so keep it outside the repository.
//!
//! The same binary is the run's two hooks: `subs-prove run-on` (SUBS_RUN_CMD: account `$LABEL`'s cell `run` with the
//! test key's one-use grant for `$TASK`) and `subs-prove window` (SUBS_WINDOW_CMD: count one window start).
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const UNITS: [&str; 6] = ["ls2-a", "ls2-b", "ls2-mac", "ls2-s0", "ls2-s1", "ls2-s2"];
const SANDBOX: &str = r#"systemd-run --user --quiet --wait --pipe --collect -p CPUQuota=10% -p MemoryMax=64M --setenv=TASK="$TASK" sh -c 'printf "task bytes: %s\n" "$(printf %s "$TASK" | wc -c)"; uname -m'"#;

fn bin(name: &str) -> PathBuf {
    let me = std::env::current_exe().expect("current exe");
    me.with_file_name(name)
}

fn env_dir() -> PathBuf {
    PathBuf::from(std::env::var("SUBS_PROVE_DIR").expect("SUBS_PROVE_DIR (set by subs-prove)"))
}

fn out(c: &mut Command) -> String {
    match c.output() {
        Ok(o) => format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).replace('\n', ""),
        Err(e) => format!("{{\"err\":\"{e}\"}}"),
    }
}

/// A reply as JSON (the call's text from its first `{`), or the text itself.
fn reply(text: &str) -> Value {
    let t = text.find('{').map(|i| &text[i..]).unwrap_or(text);
    serde_json::from_str(t).unwrap_or_else(|_| json!(text))
}

fn seed(dir: &Path) -> String {
    let k: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("test-key.json")).expect("test key")).expect("test key json");
    k["seed"].as_str().unwrap_or_default().to_string()
}

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
}

/// SUBS_RUN_CMD: one task on account `$LABEL` through that account cell's `run`, with a one-use grant for it.
fn run_on() -> i32 {
    let dir = env_dir();
    let label = std::env::var("LABEL").unwrap_or_default();
    if dir.join(format!("limit-{label}")).exists() {
        println!("{{\"limited\":true}}");
        return 0;
    }
    let body = json!({ "text": std::env::var("TASK").unwrap_or_default() }).to_string();
    let t = out(Command::new(bin("account"))
        .args(["call", "run", &body, "--grant", "--key", &format!("r{label}{}", now_ms())])
        .env("SEVEN_LOCK", dir.join(format!("{label}.lock")))
        .env("SEVEN_OWNER_SEED", seed(&dir)));
    println!("{}", t.find('{').map(|i| &t[i..]).unwrap_or(&t));
    0
}

/// SUBS_WINDOW_CMD: count the start (the record shows how many the scheduler asked for).
fn window() -> i32 {
    use std::io::Write;
    let label = std::env::var("LABEL").unwrap_or_default();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(env_dir().join("starts.log")).expect("starts.log");
    let _ = writeln!(f, "{label}");
    println!("{{\"started\":true}}");
    0
}

struct Stop;
impl Drop for Stop {
    fn drop(&mut self) {
        let _ = Command::new("systemctl").args(["--user", "stop"]).args(UNITS).status();
    }
}

fn starts(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("starts.log")).map(|s| s.lines().count()).unwrap_or(0)
}

fn run(dir: &Path) -> Result<(), String> {
    for b in ["account", "subs-cell", "subs"] {
        if !bin(b).exists() {
            return Err(format!("build first: cargo build --release ({} missing)", bin(b).display()));
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;
    let key = Command::new(bin("account")).arg("keygen").output().map_err(|e| e.to_string())?.stdout;
    std::fs::write(dir.join("test-key.json"), &key).map_err(|e| e.to_string())?;
    let k: Value = serde_json::from_slice(&key).map_err(|e| e.to_string())?;
    let (root, seed) = (k["public"].as_str().unwrap_or_default().to_string(), k["seed"].as_str().unwrap_or_default().to_string());
    let agent = "a7".repeat(32);
    let host = String::from_utf8_lossy(&Command::new("hostname").output().map_err(|e| e.to_string())?.stdout).trim().to_string();
    let acct_lock = |node: &str, port: u16, vendor: &str| format!("[cell]\nname = \"account\"\nroot = \"{root}\"\n[[replica]]\nnode = \"{node}\"\naddr = \"127.0.0.1:{port}\"\nvendors = [\"{vendor}\"]\n");
    let w = |f: &str, s: String| std::fs::write(dir.join(f), s).map_err(|e| e.to_string());
    w("a.lock", acct_lock(&host, 7591, "claude"))?;
    w("b.lock", acct_lock("sim-b", 7592, "claude"))?;
    w("mac-like.lock", acct_lock(&host, 7593, "gemini"))?;
    let mut subs_lock = format!("[cell]\nname = \"subs\"\nroot = \"{root}\"\n");
    for i in 0..3 {
        subs_lock += &format!("[[replica]]\nnode = \"r{i}\"\naddr = \"127.0.0.1:76{i}1\"\n");
    }
    w("subs.lock", subs_lock)?;
    let _stop = Stop;
    let home = std::env::var("HOME").unwrap_or_default();
    let acct = |unit: &str, lock: &str, device: &str, usage: &str| {
        Command::new("systemd-run")
            .args(["--user", "--quiet", "--unit", unit, "-p", "CPUQuota=10%", "-p", "MemoryMax=128M"])
            .arg(format!("--setenv=SEVEN_LOCK={}", dir.join(lock).display()))
            .arg(format!("--setenv=SEVEN_DEVICE={device}"))
            .arg("--setenv=SEVEN_AI=local")
            .arg(format!("--setenv=ACCOUNT_SIM_USE={usage}"))
            .arg(format!("--setenv=ACCOUNT_SANDBOX_CMD={SANDBOX}"))
            .arg(format!("--setenv=HOME={home}"))
            .arg(bin("account"))
            .args(["serve", "--dir"])
            .arg(dir.join(format!("d-{unit}")))
            .status()
            .map_err(|e| e.to_string())
    };
    let u = |pct: u32| json!({ "vendor": "claude", "window_ends": null, "week_pct": pct, "limited_until": null, "ok": true }).to_string();
    acct("ls2-a", "a.lock", &host, &u(22))?;
    acct("ls2-b", "b.lock", "sim-b", &u(61))?;
    acct("ls2-mac", "mac-like.lock", &host, &u(0))?;
    std::thread::sleep(Duration::from_secs(2));
    let acall = |lock: &str, op: &str, body: &str, key: &str| out(Command::new(bin("account")).args(["call", op, body, "--key", key]).env("SEVEN_LOCK", dir.join(format!("{lock}.lock"))).env("SEVEN_OWNER_SEED", &seed));
    let usage = |l: &str| reply(&acall(l, "usage", "null", &format!("u{l}{}", now_ms())))["reply"]["out"].clone();
    let registry = json!({ "accounts": [
        { "name": "a", "device": "device-a", "sim": true, "use": usage("a") },
        { "name": "b", "device": "device-b", "sim": true, "use": usage("b") } ] });
    w("registry.json", registry.to_string())?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    for i in 0..3 {
        Command::new("systemd-run")
            .args(["--user", "--quiet", "--unit", &format!("ls2-s{i}"), "-p", "CPUQuota=10%", "-p", "MemoryMax=128M"])
            .arg(format!("--setenv=SEVEN_LOCK={}", dir.join("subs.lock").display()))
            .arg("--setenv=SEVEN_AI=local")
            .arg(format!("--setenv=SUBS_PROVE_DIR={}", dir.display()))
            .arg(format!("--setenv=SUBS_REGISTRY_CMD=cat {}", dir.join("registry.json").display()))
            .arg(format!("--setenv=SUBS_RUN_CMD={} run-on", me.display()))
            .arg(format!("--setenv=SUBS_WINDOW_CMD={} window", me.display()))
            .arg(format!("--setenv=HOME={home}"))
            .arg(bin("subs-cell"))
            .args(["serve", "--me", &i.to_string(), "--dir"])
            .arg(dir.join(format!("s{i}")))
            .status()
            .map_err(|e| e.to_string())?;
    }
    std::thread::sleep(Duration::from_secs(3));
    let scall = |rights: &str, args: &[&str]| {
        reply(&out(Command::new(bin("subs-cell"))
            .arg("call")
            .args(args)
            .env("SEVEN_LOCK", dir.join("subs.lock"))
            .env("SEVEN_OWNER_SEED", &seed)
            .env("SEVEN_AGENT_SEED", &agent)
            .env("SEVEN_RIGHTS", rights)))
    };
    let task = r#"{"text":"summarise the failing test in repo:shop"}"#;
    let at = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let mut steps = vec![json!({ "step": "start", "at": at(), "replicas": 3, "accounts": { "a": "sim, this device", "b": "sim, a second device the mesh does not have here" } })];
    steps.push(json!({ "step": "windows", "reply": scall("subs:read", &["windows", "null", "--key", "w1"]) }));
    let needs = scall("subs:*", &["submit", task, "--key", "t1"]);
    steps.push(json!({ "step": "submit_without_tap", "reply": needs }));
    let t = Instant::now();
    let grant = Command::new(bin("subs-cell")).arg("grant").arg(needs.to_string()).env("SEVEN_DEVICE_SEED", &seed).output().map_err(|e| e.to_string())?.stdout;
    w("grant.json", String::from_utf8_lossy(&grant).to_string())?;
    steps.push(json!({ "step": "tap", "ms": t.elapsed().as_millis() as u64, "signed_by": "the test key (stands in for the owner's device key)" }));
    let t = Instant::now();
    let grant_file = dir.join("grant.json").display().to_string();
    let r = scall("subs:*", &["submit", task, "--key", "t1", "--grant-file", &grant_file]);
    steps.push(json!({ "step": "submit_with_grant", "ms": t.elapsed().as_millis() as u64, "reply": r }));
    w("limit-a", String::new())?;
    steps.push(json!({ "step": "forced_limit", "account": "a" }));
    steps.push(json!({ "step": "submit_after_limit", "reply": scall("subs:*", &["submit", r#"{"text":"open a pull request for the fix"}"#, "--key", "t2", "--grant"]) }));
    let _ = std::fs::remove_file(dir.join("limit-a"));
    let same = r#"{"accounts":[{"name":"a","device":"d","use":{"vendor":"claude","window_ends":null,"week_pct":1,"limited_until":null,"ok":true}},{"name":"c","device":"d","use":{"vendor":"claude","window_ends":null,"week_pct":1,"limited_until":null,"ok":true}}]}"#;
    let mut c = Command::new(bin("subs")).arg("windows").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        let _ = c.stdin.take().map(|mut i| i.write_all(same.as_bytes()));
    }
    let o = c.wait_with_output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).replace(['\n', '"'], "");
    steps.push(json!({ "step": "second_account_same_device", "reply": text }));
    steps.push(json!({ "step": "claude_on_a_device_row_without_it", "reply": reply(&acall("mac-like", "usage", "null", "m1")) }));
    steps.push(json!({ "step": "plan_off", "reply": scall("subs:*", &["set_plan", r#"{"account":"a","schedule":"Off","ceiling_pct":85,"starts_per_day":1}"#, "--key", "p1", "--grant"]) }));
    let _ = scall("subs:*", &["submit", r#"{"text":"a task no account serves","vendor":"gemini"}"#, "--key", "t3", "--grant"]);
    steps.push(json!({ "step": "tick_off", "reply": scall("subs:*", &["tick", "null", "--key", "k1"]), "starts": starts(&dir) }));
    steps.push(json!({ "step": "plan_queued", "reply": scall("subs:*", &["set_plan", r#"{"account":"a","schedule":"Queued","ceiling_pct":85,"starts_per_day":1}"#, "--key", "p2", "--grant"]) }));
    steps.push(json!({ "step": "tick_queued", "reply": scall("subs:*", &["tick", "null", "--key", "k2"]), "starts": starts(&dir) }));
    steps.push(json!({ "step": "tasks_own", "reply": scall("subs:read", &["tasks", "{}", "--key", "q1"]) }));
    steps.push(json!({ "step": "end", "at": at() }));
    let lines: Vec<String> = steps.iter().map(|s| s.to_string()).collect();
    w("run2.jsonl", lines.join("\n") + "\n")?;
    println!("{}", lines.join("\n"));
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("run-on") => run_on(),
        Some("window") => window(),
        Some(d) if !d.starts_with('-') => match run(Path::new(d)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        _ => {
            eprintln!("usage: subs-prove <run dir outside the repository> | run-on | window");
            2
        }
    };
    std::process::exit(code);
}
