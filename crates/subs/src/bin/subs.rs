//! `subs windows | pick [--ceiling N] [--vendor V] | due <schedule> <waiting>`: the gateway's decisions over a
//! registry read as JSON on stdin (`{"accounts": [...]}`), so any caller (a cell, a card, a script) gets the same answer.
use std::io::Read;
use subs::*;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let flag = |n: &str| a.iter().position(|x| x == n).and_then(|i| a.get(i + 1).cloned());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let reg: Registry = match serde_json::from_str::<Registry>(&input) {
        Ok(r) => {
            // re-add through the rule, so a hand-written registry cannot hold two accounts on one device
            let mut checked = Registry::default();
            for acc in r.accounts {
                if let Err(e) = checked.add(acc) {
                    eprintln!("refused: {e}");
                    std::process::exit(2);
                }
            }
            checked
        }
        Err(e) => {
            eprintln!("stdin is not a registry: {e}");
            std::process::exit(2);
        }
    };
    let out = match a.first().map(String::as_str) {
        Some("windows") => serde_json::to_value(reg.windows(now)).unwrap(),
        Some("pick") => {
            let p = Policy { ceiling: flag("--ceiling").and_then(|c| c.parse().ok()).unwrap_or(85) };
            match pick(&reg, &flag("--vendor").unwrap_or_else(|| "claude".into()), p, now) {
                Ok(p) => serde_json::to_value(p).unwrap(),
                Err(why) => serde_json::json!({ "none": why }),
            }
        }
        _ => {
            eprintln!("usage: subs windows | pick [--ceiling N] [--vendor V]  (registry JSON on stdin)");
            std::process::exit(2);
        }
    };
    println!("{out}");
}
