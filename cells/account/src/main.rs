#[ce::cell(name = "account", version = 1, home = Device, skill = "One subscription login on this device: its usage, and runs of the vendor's own CLI.")]
#[record(cell = account)] struct Seen { usage: account::Use }
#[op(cap = "account.read")] async fn usage(cx: Cx, _q: ()) -> Result<account::Use> {
    let usage = account::login_usage()?; cx.twin().put(&Seen { usage: usage.clone() }).await?; Ok(usage) }
#[op(cap = "account.run", grant = Passkey)] async fn run(cx: Cx, t: account::Task) -> Result<account::Value> {
    cx.step("account.run", || async { account::sandbox_start(&t) }).await }
