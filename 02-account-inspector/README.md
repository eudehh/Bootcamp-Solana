# 02 — Account Inspector

Pull a real on-chain account, decode every field, and verify the rent-exemption
math against the cluster's own `SysvarRent` and `SysvarClock`.

    ./inspect-account.py <PUBKEY> [--url devnet|mainnet-beta|localhost|<rpc url>]

Read-only. Needs no keypair.

## Nothing about rent is hardcoded

The script fetches `SysvarRent111111111111111111111111111111111` and decodes its
17 bytes as `<QdB` — `lamports_per_byte_year: u64`, `exemption_threshold: f64`,
`burn_percent: u8` — then does the arithmetic itself:

    (128 overhead + space) * lamports_per_byte_year * exemption_threshold

## The rent constants everyone quotes are wrong on live clusters

Measured 2026-09-07:

| cluster | lamports/byte-year | threshold | cost per byte | rent(0 bytes) |
|---|---|---|---|---|
| mainnet-beta | 6333 | **1.0** | 6333 | **810,624** |
| devnet | 5080 | **1.0** | 5080 | **650,240** |
| fresh local validator | — | — | 6960 (= 3480 × 2.0) | 890,880 |

The familiar `890880` / `3480` / threshold `2.0` are the **genesis defaults a local
validator boots with**. Both live clusters have moved off them: the threshold is
`1.0`, and the per-byte rate differs per cluster. Always ask the cluster.

## Three cross-checks

1. The computed minimum vs. `solana rent <space>` — the RPC's own answer.
2. `ACCOUNT_STORAGE_OVERHEAD` is not assumed either. It is derived empirically as
   `rent(0) / (rent(1) - rent(0))` and asserted to equal 128.
3. The account's actual balance vs. the computed minimum, reporting the surplus.

## What decoding real accounts turns up

- **The Rent sysvar verifies itself.** Inspect it and the 17 bytes it prints are
  the same values used to prove its own balance clears its own bar.
- **`rent_epoch` has two meanings.** `18446744073709551615` (`u64::MAX`) is the
  "exempt, never collect" sentinel. But the SPL Token programdata account reports
  `rent_epoch: 0` against a current epoch of 1145 while being comfortably exempt —
  the field is only rewritten when the account is touched. It is stale metadata,
  not a source of truth.
- **A program account is a pointer, not code.** `Tokenkeg…` holds 36 bytes:
  `02 00 00 00` (upgradeable-loader enum variant 2) plus a 32-byte pubkey. Follow
  it and you land on a 108,645-byte account with `7f 45 4c 46` (`ELF` magic) at
  offset `0x2d`. That one holds the bytecode — and its `executable` flag is
  `false`. The account holding the code is not the executable one.

## Try it

    ./inspect-account.py SysvarRent111111111111111111111111111111111
    ./inspect-account.py TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
    ./inspect-account.py 11111111111111111111111111111111 -u mainnet-beta
