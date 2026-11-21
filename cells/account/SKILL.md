# account (v1)

One subscription login on this device: its usage, and runs of the vendor's own CLI.

Runs as one replica on the device cell.lock names (home = Device). Generated from `account describe`; do not edit by hand.

## Ops

### `run`

- cap `account.run` (chain right `account:run`)
- input `account :: Task`, answers `account::Value`
- grant = Passkey: without a grant it answers `NeedsGrant { cell, op, caller, input_hash, summary }`; the owner's tap signs a one-use grant for that exact input (`grant`), and the same call is retried with it (`--grant-file`)
- call: `account call run '<account :: Task as JSON>' --key <idempotency key> --grant-file <grant.json>`

### `usage`

- cap `account.read` (chain right `account:read`)
- input `()`, answers `account::Use`
- call: `account call usage '<() as JSON>' --key <idempotency key>`

### `window_start`

- cap `account.window` (chain right `account:window`)
- input `()`, answers `account::Value`
- call: `account call window_start '<() as JSON>' --key <idempotency key>`

## Records

- `Seen` { usage: account :: Use }
- `Started` { day: u64, result: account :: Value }

## Refusals

- `Denied(why)`: the caller's chain does not hold the right (an audit row)
- `NeedsGrant(..)`: a Passkey op without the owner's one-use grant
- `NotLeader(addr)`, `NoQuorum`: retry with the same key; it runs once
- `Quota { agent, used, limit }`: the cell's AI bucket for that agent is spent today
- `Backend(why)`, `Input(why)`
