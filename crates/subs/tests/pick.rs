use subs::*;

const NOW: u64 = 1_800_000_000;

fn acct(name: &str, device: &str, pct: u8) -> Account {
    Account { name: name.into(), device: device.into(), usage: Use { vendor: "claude".into(), window_ends: Some(NOW + 86_400), week_pct: pct, limited_until: None, ok: true }, sim: false }
}

fn reg(a: Vec<Account>) -> Registry {
    let mut r = Registry::default();
    for x in a {
        r.add(x).unwrap();
    }
    r
}

#[test]
fn most_headroom_wins_and_the_reason_names_the_others() {
    let r = reg(vec![acct("a", "d1", 40), acct("b", "d2", 10)]);
    let p = pick(&r, "claude", Policy::default(), NOW).unwrap();
    assert_eq!(p.account, "b");
    assert!(p.why.contains("b: 10%") && p.why.contains("a: 40%"), "{}", p.why);
}

#[test]
fn a_limited_account_is_skipped_and_the_reason_says_so() {
    let mut r = reg(vec![acct("a", "d1", 40), acct("b", "d2", 10)]);
    let mut u = r.accounts[1].usage.clone();
    u.limited_until = Some(NOW + 600);
    r.set_use("b", u);
    let p = pick(&r, "claude", Policy::default(), NOW).unwrap();
    assert_eq!(p.account, "a");
    assert!(p.why.contains("skipped b: limited for 10 more min"), "{}", p.why);
    // once the limit has passed, b is back
    assert_eq!(pick(&r, "claude", Policy::default(), NOW + 601).unwrap().account, "b");
}

#[test]
fn the_ceiling_leaves_an_account_alone_and_none_left_is_an_error_with_reasons() {
    let r = reg(vec![acct("a", "d1", 85), acct("b", "d2", 97)]);
    let e = pick(&r, "claude", Policy::default(), NOW).unwrap_err();
    assert!(e.contains("a: 85% of the week used, ceiling 85%") && e.contains("b: 97%"), "{e}");
    assert_eq!(pick(&r, "claude", Policy { ceiling: 90 }, NOW).unwrap().account, "a");
}

#[test]
fn an_account_that_is_not_ok_is_skipped() {
    let mut r = reg(vec![acct("a", "d1", 50), acct("b", "d2", 5)]);
    let mut u = r.accounts[1].usage.clone();
    u.ok = false;
    r.set_use("b", u);
    assert_eq!(pick(&r, "claude", Policy::default(), NOW).unwrap().account, "a");
}

#[test]
fn a_second_account_on_one_device_is_refused() {
    let mut r = reg(vec![acct("a", "d1", 0)]);
    let e = r.add(acct("b", "d1", 0)).unwrap_err();
    assert_eq!(e, Refused::DeviceTaken { device: "d1".into(), vendor: "claude".into(), by: "a".into() });
    // another vendor on the same device is a different account
    let mut other = acct("c", "d1", 0);
    other.usage.vendor = "codex".into();
    r.add(other).unwrap();
}

#[test]
fn off_makes_zero_asks_whatever_waits() {
    for waiting in [0, 1, 50] {
        for open in [false, true] {
            for minute in [0u16, 480, 1439] {
                assert_eq!(starts_due(&Schedule::Off, waiting, open, minute), 0);
            }
        }
    }
    assert_eq!(Schedule::default(), Schedule::Off);
    assert_eq!(starts_due(&Schedule::Queued, 0, false, 0), 0);
    assert_eq!(starts_due(&Schedule::Queued, 2, false, 0), 1);
    assert_eq!(starts_due(&Schedule::Queued, 2, true, 0), 0);
    assert_eq!(starts_due(&Schedule::Fixed(vec![480]), 0, false, 480), 1);
    assert_eq!(starts_due(&Schedule::Fixed(vec![480]), 0, false, 481), 0);
}

#[test]
fn the_gateway_line_parses() {
    let u = Use::from_reported("claude", "seven_day window: 12% used, status allowed, resets at unix 1795683600", 0);
    assert_eq!((u.week_pct, u.window_ends, u.limited_until, u.ok), (12, Some(1795683600), None, true));
    let l = Use::from_reported("claude", "seven_day window: 100% used, status rejected, resets at unix 1795683600", 0);
    assert_eq!(l.limited_until, Some(1795683600));
    assert!(!Use::from_reported("claude", "", 0).ok);
}

#[test]
fn windows_show_every_account() {
    let r = reg(vec![acct("a", "d1", 40), acct("b", "d2", 10)]);
    let w = r.windows(NOW);
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].resets_in_s, Some(86_400));
}
