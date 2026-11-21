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

## Run 2 (2026-11-21): the subs cell

Record: `docs/runs/2026-11-21-run2.json`. The subs cell on 3 replicas over two account cells,
each process under `systemd-run --user` with CPUQuota=10%, all stopped at the end. Windows answer bands to a caller
without `subs:usage`; a submit without a tap answers `NeedsGrant`; the one-use grant for that exact task runs it on the
account with the most headroom and stores why; a forced limit moves the next task with both reasons; a second account
on one device is refused; Claude on a device row that does not list it is `NotOnThisDevice`; scheduler `Off` makes 0
window starts and `Queued` exactly 1.

## Not done yet

- Runs use a stand-in task in a `systemd-run` sandbox. `run` asks `ai.account Lease` first (`ACCOUNT_LEASE_CMD`), but
  the account cell holds no `agent.box:start` grant yet, so no vendor CLI ran.
- The second device is sim: run 2 ran on one server.
- The card on the owner's phone that answers a `NeedsGrant` is not wired; a fresh test key signed the grants.
- No Tasks card in the control room yet; T3 Code has a catalog entry (ce-net/wrap) but no instance behind wrap.auth.
- The 10-of-30 chain-quota proof ran through a real ce-gate in sub.gateway's tests; the deployed gateway gets it with
  its next install (sub-gateway HANDOFF.md).
- Real accounts stay scheduler `Off` until the owner turns one on.
- The run 2 recorder was a shell driver (kept outside the repository: new code here is Rust); it becomes a `prove`
  subcommand of the subs cell, as the seven-lines examples have.

Licence: Apache-2.0.
