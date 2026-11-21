#!/bin/sh
# The live-subs proof: two account cells (one per device), pick, a granted run in a CPU-capped sandbox, a forced
# limit, the move to the other account, and the reason each time. Everything it starts is stopped at the end.
#
#   RUN_DIR       where locks, keys and the raw record go (outside the repository: locks name devices)
#   A_USAGE_CMD   prints account a's usage (a `Use`, or sub.gateway's window line); unset = a is sim too
#   SANDBOX_CMD   the sandbox a run starts in, gets the task in $TASK (default: systemd-run --user, CPUQuota=10%)
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
ACC=$ROOT/target/release/account; SUBS=$ROOT/target/release/subs
[ -x "$ACC" ] && [ -x "$SUBS" ] || { echo "build first: cargo build --release" >&2; exit 1; }
RUN_DIR=${RUN_DIR:?set RUN_DIR outside the repository}; mkdir -p "$RUN_DIR"; cd "$RUN_DIR"
SANDBOX_CMD=${SANDBOX_CMD:-'systemd-run --user --quiet --wait --pipe --collect -p CPUQuota=10% -p MemoryMax=64M --setenv=TASK="$TASK" sh -c '"'"'printf "task bytes: %s\n" "$(printf %s "$TASK" | wc -c)"; uname -m'"'"''}
now() { date +%s; }
ms() { echo $(( $(date +%s%N) / 1000000 )); }
"$ACC" keygen > owner.json
ROOTKEY=$(sed 's/.*"public":"\([0-9a-f]*\)".*/\1/' owner.json); SEED=$(sed 's/.*"seed":"\([0-9a-f]*\)".*/\1/' owner.json)
A_NODE=$( [ -n "${A_USAGE_CMD:-}" ] && hostname || echo sim-a )
printf '[cell]\nname = "account"\nroot = "%s"\n[[replica]]\nnode = "%s"\naddr = "127.0.0.1:7491"\n' "$ROOTKEY" "$A_NODE" > a.lock
printf '[cell]\nname = "account"\nroot = "%s"\n[[replica]]\nnode = "sim-b"\naddr = "127.0.0.1:7492"\n' "$ROOTKEY" > b.lock
SIM_A='{"vendor":"claude","window_ends":null,"week_pct":20,"limited_until":null,"ok":true}'
start_b() { # $1 = the sim usage of b
  systemctl --user stop live-subs-b 2>/dev/null || true
  systemd-run --user --quiet --unit live-subs-b -p CPUQuota=10% -p MemoryMax=128M --setenv=SEVEN_LOCK="$RUN_DIR/b.lock" --setenv=SEVEN_DEVICE=sim-b \
    --setenv=SEVEN_AI=local --setenv=ACCOUNT_SIM_USE="$1" --setenv=ACCOUNT_SANDBOX_CMD="$SANDBOX_CMD" --setenv=HOME="$HOME" \
    "$ACC" serve --dir "$RUN_DIR/db"
}
systemd-run --user --quiet --unit live-subs-a -p CPUQuota=10% -p MemoryMax=128M --setenv=SEVEN_LOCK="$RUN_DIR/a.lock" --setenv=SEVEN_AI=local \
  --setenv=PATH="$PATH" --setenv=HOME="$HOME" ${A_USAGE_CMD:+--setenv=ACCOUNT_USAGE_CMD="$A_USAGE_CMD"} --setenv=ACCOUNT_SIM_USE="$SIM_A" \
  --setenv=ACCOUNT_SANDBOX_CMD="$SANDBOX_CMD" "$ACC" serve --dir "$RUN_DIR/da"
start_b '{"vendor":"claude","window_ends":null,"week_pct":0,"limited_until":null,"ok":true}'
trap 'systemctl --user stop live-subs-a live-subs-b 2>/dev/null || true' EXIT
sleep 2
call() { SEVEN_LOCK="$RUN_DIR/$1.lock" SEVEN_OWNER_SEED=$SEED "$ACC" call "$2" "$3" ${4:-} 2>&1; }
usage() { call "$1" usage null | tr -d '\n' | sed 's/.*"out": *\({[^}]*}\).*/\1/'; }
registry() { printf '{"accounts":[{"name":"a","device":"device-a","sim":%s,"use":%s},{"name":"b","device":"sim-b","sim":true,"use":%s}]}' \
  "$( [ -n "${A_USAGE_CMD:-}" ] && echo false || echo true )" "$(usage a)" "$(usage b)"; }
TASK='{"text":"summarise the failing test in repo:shop"}'
{
  echo "{\"step\":\"start\",\"at\":$(now),\"a\":\"$( [ -n "${A_USAGE_CMD:-}" ] && echo live || echo sim )\",\"b\":\"sim\"}"
  R1=$(registry); echo "{\"step\":\"registry\",\"r\":$R1}"
  echo "{\"step\":\"windows\",\"w\":$(echo "$R1" | "$SUBS" windows)}"
  t=$(ms); P1=$(echo "$R1" | "$SUBS" pick); echo "{\"step\":\"pick\",\"ms\":$(( $(ms) - t )),\"p\":$P1}"
  W1=$(echo "$P1" | sed 's/.*"account":"\([a-z]*\)".*/\1/')
  echo "{\"step\":\"run_without_grant\",\"account\":\"$W1\",\"reply\":$(call "$W1" run "$TASK" | tail -1 | sed 's/^/"/; s/$/"/; s/\\/\\\\/g')}" 
  t=$(ms); OUT=$(call "$W1" run "$TASK" --grant | tr -d '\n'); echo "{\"step\":\"run\",\"account\":\"$W1\",\"ms\":$(( $(ms) - t )),\"reply\":$OUT}"
  start_b "{\"vendor\":\"claude\",\"window_ends\":null,\"week_pct\":100,\"limited_until\":$(( $(now) + 1800 )),\"ok\":true}"; sleep 2
  echo "{\"step\":\"forced_limit\",\"account\":\"b\",\"limited_for_s\":1800}"
  R2=$(registry); t=$(ms); P2=$(echo "$R2" | "$SUBS" pick); echo "{\"step\":\"pick_after_limit\",\"ms\":$(( $(ms) - t )),\"p\":$P2}"
  W2=$(echo "$P2" | sed 's/.*"account":"\([a-z]*\)".*/\1/')
  t=$(ms); OUT=$(call "$W2" run "$TASK" --grant | tr -d '\n'); echo "{\"step\":\"run\",\"account\":\"$W2\",\"ms\":$(( $(ms) - t )),\"reply\":$OUT}"
  echo "{\"step\":\"end\",\"at\":$(now)}"
} > run.jsonl
cat run.jsonl
