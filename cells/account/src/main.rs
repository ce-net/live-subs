#[ce::cell(name = "account", version = 1, home = Device, skill = "One subscription login on this device: its usage, and runs of the vendor's own CLI.")]
#[record(cell = account)] struct Seen { usage: account::Use }
#[record(cell = account)] struct Started { day: u64, result: account::Value }
#[op(cap = "account.read")] async fn usage(cx: Cx, _q: ()) -> Result<account::Use> {
    let usage = account::login_usage()?; cx.twin().put(&Seen { usage: usage.clone() }).await?; Ok(usage) }
#[op(cap = "account.run", grant = Passkey)] async fn run(cx: Cx, t: account::Task) -> Result<account::Value> {
    cx.step("account.run", || async { account::sandbox_start(&t) }).await }
#[op(cap = "account.window")] async fn window_start(cx: Cx, _q: ()) -> Result<account::Value> {
    let (u, day) = (account::login_usage()?, account::today()); let n = cx.twin().all::<Started>().iter().filter(|(_, s)| s.day == day).count() as u32;
    let result = cx.step("account.window", || async { account::window_start(&u, n) }).await?; cx.twin().put(&Started { day, result: result.clone() }).await?; Ok(result) }
