#!/usr/bin/env bash
# Builds the program, boots a validator with it preloaded, runs the client.
set -euo pipefail
LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$LAB/.work"; mkdir -p "$WORK"

echo "building the program..."
if ! cargo-build-sbf --manifest-path "$LAB/program/Cargo.toml" >"$WORK/build.log" 2>&1; then
  grep -E '^error' -A8 "$WORK/build.log" || cat "$WORK/build.log"
  exit 1
fi
SO="$LAB/program/target/deploy/counter_program.so"
KP="$LAB/program/target/deploy/counter_program-keypair.json"
PROGRAM_ID="$(solana address -k "$KP")"
echo "program id: $PROGRAM_ID"

PID=""
cleanup() { [[ -n "$PID" ]] && { kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; }; }
trap cleanup EXIT

# --bpf-program loads it at genesis, so there is no deploy step and the id is stable.
solana-test-validator --ledger "$WORK/ledger" --reset --quiet \
  --bpf-program "$PROGRAM_ID" "$SO" >"$WORK/validator.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do
  solana -u localhost cluster-version >/dev/null 2>&1 && break
  sleep 0.5
done

cargo run --quiet --manifest-path "$LAB/client/Cargo.toml" -- "http://127.0.0.1:8899" "$PROGRAM_ID"
