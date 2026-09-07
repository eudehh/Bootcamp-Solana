#!/usr/bin/env python3
"""
inspect-account.py — pull a real on-chain account, decode every field, and check
the rent-exemption math against the cluster's own Rent and Clock sysvars.

Nothing about rent is hardcoded. The classic "890880 lamports / 3480 per byte-year
/ threshold 2.0" numbers are mainnet's; other clusters differ, so every constant
here is read from SysvarRent and then cross-checked against the RPC's own answer.

  ./inspect-account.py <PUBKEY> [--url devnet|mainnet-beta|localhost|<rpc url>]
"""
import argparse, base64, json, struct, subprocess, sys

SYSVAR_RENT  = "SysvarRent111111111111111111111111111111111"
SYSVAR_CLOCK = "SysvarC1ock11111111111111111111111111111111"
U64_MAX      = 2**64 - 1
STORAGE_OVERHEAD = 128   # ACCOUNT_STORAGE_OVERHEAD — asserted below, not trusted

KNOWN = {
    "11111111111111111111111111111111":            "System Program",
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA": "SPL Token",
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb": "SPL Token-2022",
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL":"Associated Token Account",
    "BPFLoaderUpgradeab1e11111111111111111111111": "BPF Upgradeable Loader",
    "Sysvar1111111111111111111111111111111111111": "Sysvar",
    "Stake11111111111111111111111111111111111111": "Stake Program",
    "Vote111111111111111111111111111111111111111": "Vote Program",
    "ComputeBudget111111111111111111111111111111": "Compute Budget",
}

B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58(b):
    n = int.from_bytes(b, 'big'); s = ''
    while n:
        n, r = divmod(n, 58); s = B58[r] + s
    return '1' * (len(b) - len(b.lstrip(b'\0'))) + s

def sol(url, *args):
    cmd = ["solana", *args, "--output", "json"]
    if url: cmd += ["-u", url]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"error: solana {' '.join(args)} failed:\n{p.stderr.strip()}")
    return json.loads(p.stdout)

def data_of(acct):
    d = acct["account"]["data"]
    return base64.b64decode(d[0] if isinstance(d, list) else d)

def lamports(n):
    return f"{n:,} lamports ({n/1e9:.9f} SOL)"

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pubkey")
    ap.add_argument("--url", "-u", default=None, help="cluster (default: CLI config)")
    a = ap.parse_args()

    acct = sol(a.url, "account", a.pubkey)["account"]
    data = data_of({"account": acct})
    space = acct["space"]

    # ---- the cluster's live parameters -------------------------------------
    rent_raw  = data_of(sol(a.url, "account", SYSVAR_RENT))
    lpby, threshold, burn = struct.unpack("<QdB", rent_raw[:17])
    clock_raw = data_of(sol(a.url, "account", SYSVAR_CLOCK))
    slot, epoch_start_ts, epoch, leader_epoch, unix_ts = struct.unpack("<QqQQq", clock_raw[:40])

    print(f"\n\033[1mACCOUNT\033[0m  {a.pubkey}")
    print(f"  lamports    {lamports(acct['lamports'])}")
    owner = acct["owner"]
    print(f"  owner       {owner}" + (f"   <- {KNOWN[owner]}" if owner in KNOWN else ""))
    print(f"              only this program may write the data or debit the lamports")
    print(f"  executable  {acct['executable']}" + ("   <- this account is code" if acct["executable"] else ""))
    re = acct["rentEpoch"]
    print(f"  rent_epoch  {re}" + ("   <- u64::MAX sentinel: exempt, never collect"
                                   if re == U64_MAX else f"   (current epoch is {epoch})"))
    print(f"  data        {space} bytes")
    if data:
        for off in range(0, min(len(data), 64), 16):
            chunk = data[off:off+16]
            hexs = " ".join(f"{b:02x}" for b in chunk)
            txt  = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
            print(f"              {off:04x}  {hexs:<47}  {txt}")
        if len(data) > 64:
            print(f"              ... {len(data)-64} more bytes")

    print(f"\n\033[1mCLUSTER\033[0m  (SysvarClock)")
    print(f"  epoch {epoch}, slot {slot}, leader_schedule_epoch {leader_epoch}")

    print(f"\n\033[1mRENT PARAMS\033[0m  (SysvarRent, decoded from its 17 bytes)")
    print(f"  lamports_per_byte_year  {lpby}")
    print(f"  exemption_threshold     {threshold}  ({threshold} years prepaid)")
    print(f"  burn_percent            {burn}%")

    # ---- the math ----------------------------------------------------------
    per_byte = lpby * threshold
    minimum  = int((STORAGE_OVERHEAD + space) * per_byte)
    print(f"\n\033[1mRENT-EXEMPTION MATH\033[0m")
    print(f"  ({STORAGE_OVERHEAD} overhead + {space} data) x {lpby} x {threshold} = {minimum:,} lamports")

    # cross-check 1: does the RPC agree with our arithmetic?
    rpc_min = sol(a.url, "rent", str(space))["rentExemptMinimumLamports"]
    ok = "OK " if rpc_min == minimum else "MISMATCH "
    print(f"  [{ok}] `solana rent {space}` says {rpc_min:,}")

    # cross-check 2: is the 128-byte overhead constant actually what this cluster uses?
    zero, one = (sol(a.url, "rent", str(n))["rentExemptMinimumLamports"] for n in (0, 1))
    derived_overhead = zero / (one - zero) if one != zero else float("nan")
    ok = "OK " if abs(derived_overhead - STORAGE_OVERHEAD) < 1e-9 else "MISMATCH "
    print(f"  [{ok}] overhead derived from rent(0)/(rent(1)-rent(0)) = {derived_overhead:g}")

    # cross-check 3: is THIS account actually exempt?
    bal = acct["lamports"]
    if bal >= minimum:
        print(f"  [OK ] balance {bal:,} >= minimum: exempt, surplus {bal-minimum:,} lamports")
    else:
        print(f"  [!! ] balance {bal:,} < minimum: NOT exempt, short {minimum-bal:,} lamports")
        print(f"        this account is subject to collection and can be purged")
    if re != U64_MAX and bal >= minimum:
        print(f"  note: rent_epoch is {re}, not the u64::MAX sentinel, though the balance")
        print(f"        clears the bar — the field is only rewritten when touched.")

if __name__ == "__main__":
    main()
