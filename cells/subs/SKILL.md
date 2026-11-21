# subs (v1)

Pick an account for a task and say why. Moves tasks, never logins or model traffic.

Runs as 3 replicas over one quorum log; every effect happens once. Generated from `subs describe`; do not edit by hand.

## Ops

### `set_plan`

- cap `subs.schedule` (chain right `subs:schedule`)
- input `Plan`, answers `Plan`
- grant = Passkey: without a grant it answers `NeedsGrant { cell, op, caller, input_hash, summary }`; the owner's tap signs a one-use grant for that exact input (`grant`), and the same call is retried with it (`--grant-file`)
- call: `subs call set_plan '<Plan as JSON>' --key <idempotency key> --grant-file <grant.json>`

### `submit`

- cap `subs.submit` (chain right `subs:submit`)
- input `NewTask`, answers `Task`
- grant = Passkey: without a grant it answers `NeedsGrant { cell, op, caller, input_hash, summary }`; the owner's tap signs a one-use grant for that exact input (`grant`), and the same call is retried with it (`--grant-file`)
- call: `subs call submit '<NewTask as JSON>' --key <idempotency key> --grant-file <grant.json>`

### `tasks`

- cap `subs.read` (chain right `subs:read`)
- input `TaskQuery`, answers `Vec<Task>`
- call: `subs call tasks '<TaskQuery as JSON>' --key <idempotency key>`

### `tick`

- cap `subs.schedule` (chain right `subs:schedule`)
- input `()`, answers `Vec<Start>`
- call: `subs call tick '<() as JSON>' --key <idempotency key>`

### `windows`

- cap `subs.read` (chain right `subs:read`)
- input `()`, answers `Vec<Window>`
- call: `subs call windows '<() as JSON>' --key <idempotency key>`

## Records

- `Plan` { account: String, schedule: Schedule, ceiling_pct: u8, starts_per_day: u8 }
- `Task` { id: String, by: String, text: String, repo: Option < String >, state: TaskState, account: Option < String >, why: String, result: Value }

## Refusals

- `Denied(why)`: the caller's chain does not hold the right (an audit row)
- `NeedsGrant(..)`: a Passkey op without the owner's one-use grant
- `NotLeader(addr)`, `NoQuorum`: retry with the same key; it runs once
- `Quota { agent, used, limit }`: the cell's AI bucket for that agent is spent today
- `Backend(why)`, `Input(why)`
