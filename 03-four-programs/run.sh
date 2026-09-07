#!/usr/bin/env bash
# Starts a local validator, runs the exercise against it, tears it down.
set -euo pipefail
LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$LAB/.work"; mkdir -p "$WORK"

PID=""
cleanup() { [[ -n "$PID" ]] && { kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; }; }
trap cleanup EXIT

solana-test-validator --ledger "$WORK/ledger" --reset --quiet >"$WORK/validator.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do
  solana -u localhost cluster-version >/dev/null 2>&1 && break
  sleep 0.5
done
cargo run --quiet --manifest-path "$LAB/Cargo.toml" -- "${1:-http://127.0.0.1:8899}"
