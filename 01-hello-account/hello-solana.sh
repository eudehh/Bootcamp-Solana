#!/usr/bin/env bash
#
# hello-solana.sh — create an account, fund it to rent-exemption, inspect the five fields.
#
# Self-contained: spins up its own validator, uses its own config file and its own
# keypair, and tears everything down on exit. Never reads or writes the global
# ~/.config/solana/cli/config.yml.
#
#   ./hello-solana.sh          run and tear down
#   ./hello-solana.sh --keep   leave the validator running to poke at by hand
#
set -euo pipefail

LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$LAB/.work"
CFG="$WORK/config.yml"          # isolated config — the -C flag on every command
KEY="$WORK/hello.json"          # the keypair whose address we create
LEDGER="$WORK/ledger"
ENV_OUT="$LAB/.test-env"        # addresses written here for later runs

KEEP=0
[[ "${1:-}" == "--keep" ]] && KEEP=1

for bin in solana solana-keygen spl-token solana-test-validator jq; do
  command -v "$bin" >/dev/null 2>&1 || { echo "missing dependency: $bin" >&2; exit 1; }
done

mkdir -p "$WORK"

VALIDATOR_PID=""
cleanup() {
  if [[ -n "$VALIDATOR_PID" && $KEEP -eq 0 ]]; then
    echo
    echo "tearing down validator (pid $VALIDATOR_PID)"
    kill "$VALIDATOR_PID" 2>/dev/null || true
    wait "$VALIDATOR_PID" 2>/dev/null || true
  elif [[ $KEEP -eq 1 ]]; then
    echo
    echo "validator left running (pid $VALIDATOR_PID). Talk to it with:"
    echo "  solana -C $CFG <cmd>"
    echo "  spl-token -C $CFG <cmd>"
    echo "Stop it with: kill $VALIDATOR_PID"
  fi
}
trap cleanup EXIT

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
sol()  { solana    -C "$CFG" "$@"; }
tok()  { spl-token -C "$CFG" "$@"; }

# ---------------------------------------------------------------- validator
step "starting local validator"
solana-test-validator --ledger "$LEDGER" --reset --quiet >"$WORK/validator.log" 2>&1 &
VALIDATOR_PID=$!

sol config set --url localhost >/dev/null
ready=0
for _ in $(seq 1 60); do
  if sol cluster-version >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.5
done
[[ $ready -eq 1 ]] || { echo "validator never came up; see $WORK/validator.log" >&2; exit 1; }
echo "up, running $(sol cluster-version)"

# ---------------------------------------------------------------- keypair
step "1. a keypair is just a name — no chain involved"
[[ -f "$KEY" ]] || solana-keygen new -o "$KEY" --no-bip39-passphrase --silent >/dev/null
sol config set --keypair "$KEY" >/dev/null
ME="$(sol address)"
echo "address: $ME"

echo
echo "does an account exist at that address yet?"
sol account "$ME" 2>&1 | sed 's/^/  /' || true
echo "  ^ an address is not an account. Nothing exists until someone pays for it."

# ---------------------------------------------------------------- rent
step "2. rent-exemption is the price of existing"
# Rent params are on-chain cluster state, NOT a constant. Always ask the cluster
# you are actually pointed at — devnet and a local validator can differ.
MIN=$(sol rent 0 --output json | jq -r .rentExemptMinimumLamports)
echo "rent-exempt minimum for 0 bytes of data: $MIN lamports"

echo
echo "funding with less than that (1000 lamports) — expect the tx to be rejected:"
sol airdrop 0.000001 "$ME" 2>&1 | sed 's/^/  /' || true

# ---------------------------------------------------------------- creation
step "3. fund with exactly the minimum — the account springs into existence"
sol airdrop "$(jq -rn "$MIN/1000000000")" "$ME" >/dev/null
echo "created."

step "4. the five fields"
sol account "$ME" | sed 's/^/  /'
cat <<'NOTE'

  lamports    the balance. Anyone may credit it; only the owner may debit it.
  data        absent here — a wallet holds zero bytes.
  owner       System Program. The ONLY program allowed to write these bytes.
  executable  false. This account is data, not code.
  rent_epoch  u64::MAX — the "rent exempt, never collect" sentinel.
NOTE

# ---------------------------------------------------------------- token
step "5. an account with data, and a non-System owner"
sol airdrop 2 "$ME" >/dev/null
MINT=$(tok create-token --decimals 6 --output json | jq -r .commandOutput.address)
echo "mint:          $MINT"
tok create-account "$MINT" >/dev/null
# The associated token account address is derived, not parsed out of creation
# output — create-account's JSON returns only a signature.
TA=$(tok address --verbose --token "$MINT" --output json | jq -r .associatedTokenAddress)
echo "token account: $TA"

step "6. the same account has TWO owners"
echo "-- runtime layer: solana account --"
sol account "$TA" | sed -n '1,7p' | sed 's/^/  /'
echo
echo "-- application layer: spl-token display --"
tok display "$TA" | sed 's/^/  /'

if command -v python3 >/dev/null 2>&1; then
  step "7. the application owner is just bytes in the blob"
  DATA=$(sol account "$TA" --output json | jq -r 'if (.account.data|type)=="array" then .account.data[0] else .account.data end')
  python3 - "$DATA" "$ME" <<'PY'
import base64, sys
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58(b: bytes) -> str:
    n = int.from_bytes(b, 'big'); s = ''
    while n:
        n, r = divmod(n, 58); s = ALPHABET[r] + s
    return '1' * (len(b) - len(b.lstrip(b'\0'))) + s
raw = base64.b64decode(sys.argv[1])
# SPL token account layout: mint [0:32], owner [32:64], amount [64:72], ...
print(f"  data[32:64] decodes to: {b58(raw[32:64])}")
print(f"  our keypair is:         {sys.argv[2]}")
print("  Same value. 'authority' is not a runtime field — it is a slice of the")
print("  data blob that the Token program's code chooses to enforce. Nothing")
print("  outside that program checks it.")
PY
fi

# ---------------------------------------------------------------- persist
step "addresses written to $ENV_OUT"
cat > "$ENV_OUT" <<EOF
# generated by hello-solana.sh on $(date -u '+%Y-%m-%dT%H:%M:%SZ')
CFG=$CFG
ME=$ME
MINT=$MINT
TA=$TA
EOF
cat "$ENV_OUT" | sed 's/^/  /'
