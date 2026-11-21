# live-subs: several subscriptions, done as cells

One `account` cell per device owns that device's own login. The `subs` gateway sees every account, picks one for
each task and says why. It moves tasks. It never moves a credential and never carries model traffic.

```sh
ce up account --on home-mini --as a      # one account cell per device, one device per account
ce up account --on cloud-box --as b
ce task "fix the flaky test in repo:shop"
```

The account cell, whole file ([`cells/account/src/main.rs`](cells/account/src/main.rs)), on the
[seven-lines](https://github.com/ce-net/seven-lines) runtime:

```rust
#[ce::cell(name = "account", version = 1, home = Device, skill = "One subscription login on this device: its usage, and runs of the vendor's own CLI.")]
#[record(cell = account)] struct Seen { usage: account::Use }
#[op(cap = "account.read")] async fn usage(cx: Cx, _q: ()) -> Result<account::Use> {
    let usage = account::login_usage()?; cx.twin().put(&Seen { usage: usage.clone() }).await?; Ok(usage) }
#[op(cap = "account.run", grant = Passkey)] async fn run(cx: Cx, t: account::Task) -> Result<account::Value> {
    cx.step("account.run", || async { account::sandbox_start(&t) }).await }
```

## The terms are in the structure

- `home = Device`: one replica, and `serve` refuses to start on any device except the one its lock names.
  `replicas = 3` with `home = Device` does not compile.
- One account per device per vendor: the registry refuses a second one (`Refused::DeviceTaken`).
- `account.run` is `grant = Passkey`: each run needs a grant signed by the owner key for this caller and this
  exact task. It is used once and cannot be delegated.
- There is no API-compatible endpoint, so there is nothing to share, resell or proxy. A run starts the vendor's
  unmodified CLI on the account's own device. The task text reaches the sandbox as data (`$TASK`), never as shell.

## Pick

In order: the account is `ok`, it is not limited now, and its weekly percent is under the owner's ceiling
(default 85). Of those, the one with the most weekly headroom wins. The reason names every other account and why
it was skipped, and it is stored with the task. Scheduler: `Off` by default and makes zero asks. `Queued` starts a
window only when a task waits. `Fixed` is the owner's own start times.

```sh
cargo test                       # 8 pick/schedule tests, 4 account-cell tests (device, grant, $TASK as data)
cargo build --release && RUN_DIR=/tmp/run scripts/prove.sh     # both accounts sim unless A_USAGE_CMD is set
```

## The recorded run (2026-11-21)

[`docs/runs/2026-11-21-live.json`](docs/runs/2026-11-21-live.json), page: <https://ce-net.github.io/live-subs/>.
It ran on one server with two account cells, each capped at `CPUQuota=10%`. Account `a` was live: its usage came
from the real readout sub.gateway records for that device's own login, and its percent is not published. Account
`b` was sim, standing in for a second device.

| | |
|---|---|
| pick | `b` in 7 ms: "b: 0% of the week used, the most headroom; a: …" |
| no grant | `run` without the owner's grant was refused |
| run | with the grant: 259 ms at the client, 245 ms in the sandbox (`systemd-run`, `CPUQuota=10%`) |
| forced limit | `b` limited for 30 min |
| move | the next pick took `a` in 5 ms: "skipped b: limited for 30 more min". Its run took 254 ms |
| scheduler | `Off`. The gateway's ask count for the device was the same before and after |

## Not done yet

- The run is a stand-in task in a `systemd-run` sandbox. agent.box refused this caller because it has no
  `agent.box:start` grant, and no vendor CLI was started.
- The owner key was a demo key made for the run. The passkey card on the owner's phone that signs a grant is not
  wired up yet.
- `subs` is a library and a CLI (`subs windows | pick`), not yet a replicated cell with `Submit` and `SetSchedule`
  ops. There is no Tasks card in the control room and no T3 Code wrap spec or health row.
- `cx.login().usage()` per vendor: slice 1 reads the window line sub.gateway already records.
- The second device is sim. The second node's install grant is not verified.

Licence: Apache-2.0.
