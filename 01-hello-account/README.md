# 01 — Hello Account

Create an account, fund it to rent-exemption, and inspect the five fields.

    ./hello-solana.sh          # run and tear down
    ./hello-solana.sh --keep   # leave the validator up to poke at by hand

Runs against its own `solana-test-validator` with its own config file and its own
keypair. Your global CLI config is never read or written.

## What it walks through

1. **A keypair is just a name.** `solana-keygen new` works offline and instantly.
   Querying that address returns `AccountNotFound` — nothing exists until someone
   pays for it.
2. **Rent-exemption is the price of existing.** Funding below the minimum doesn't
   create a poor account, it *fails the whole transaction*:
   `Transaction results in an account (1) with insufficient funds for rent`.
3. **The five fields**, on a freshly created wallet: 890,880 lamports, owned by the
   System Program, `executable: false`, `rent_epoch` at the `u64::MAX` sentinel,
   and zero bytes of data.
4. **Two owners on one account.** It then creates a mint and a token account, and
   prints both views side by side:

   | | value | means |
   |---|---|---|
   | `solana account` → Owner | `Tokenkeg…` | **runtime**: the only program allowed to write these 165 bytes |
   | `spl-token display` → Owner | your pubkey | **application**: whose signature the Token program chose to honor |

5. **The proof.** It decodes `data[32:64]` from the raw blob back to base58 and
   shows it equals your pubkey. The "authority" is not a field — it is bytes that
   the Token program's code chooses to enforce.

## Two things that bite

- **Rent parameters are cluster state, not constants.** Ask the cluster you are
  actually pointed at. `solana rent 0` returns a different answer on devnet than on
  a local validator, and feeding one to the other gets your transaction rejected.
- **`spl-token create-account --output json` returns only a signature**, no address.
  Derive the token account address instead — and `--verbose` is mandatory there:

      spl-token address --verbose --token <MINT> --output json | jq -r .associatedTokenAddress

## Note

`--reset` wipes the ledger every run, so the keypair file persists (stable address)
but the mint and token account are new each time. Drop `--reset` to carry them over.
