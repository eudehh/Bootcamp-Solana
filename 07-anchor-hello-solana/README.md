# 07 — Anchor: hello_solana

The `anchor init` scaffold, fixed and wired into the pack.

    anchor build     # compiles the program + generates the IDL
    anchor test      # rebuilds as SBPF v0, then runs the litesvm tests

## The scaffold does not pass its own tests out of the box

`anchor test` on a fresh `anchor init` fails immediately:

    thread 'test_initialize' panicked at tests/test_initialize.rs:29:
    called `Result::unwrap()` on an `Err` value: Instruction(InvalidAccountData)

Line 29 is `svm.add_program(...)` — it fails *loading the binary*, before any
transaction runs. The cause:

| | |
|---|---|
| `anchor build` emits | SBPF **v3** (`e_flags = 3` in the ELF header) |
| `litesvm 0.10`, pinned by the scaffold's own `Cargo.toml`, can load | not v3 |

`cargo-build-sbf` defaults to `--arch v0`, so this only happens because Anchor
overrides it. And you cannot override it back — Anchor passes `--arch` itself:

    $ anchor build -- --arch v0
    error: The argument '--arch <arch>' was provided more than once

**The fix** is in `Anchor.toml`: rebuild as v0 before running the tests.

    [scripts]
    test = "cargo-build-sbf --manifest-path programs/hello_solana/Cargo.toml \
            --arch v0 --sbf-out-dir target/deploy && cargo test"

With that, `anchor test` is green. Verified on anchor-cli 1.2.0 / solana-cli
3.1.10 / platform-tools v1.52.

## What Anchor is doing that exercise 05 did by hand

Exercise 05 is the same idea — a counter in a PDA — written natively. Put them
side by side:

| | 05 (native) | 07 (Anchor) |
|---|---|---|
| account creation | `invoke_signed` + `create_account` by hand | `#[account(init, payer, space)]` |
| PDA check | `find_program_address`, compare, reject | `seeds = [...], bump` constraint |
| signer check | `if !user.is_signer` | `Signer<'info>` type |
| owner check | `if counter.owner != program_id` | `Account<'info, Counter>` |
| layout | byte offsets you wrote | borsh + an 8-byte discriminator |
| authority check | compare stored pubkey to signer | `require_keys_eq!` |
| client interface | hand-encoded tags | generated IDL |

Anchor's `Account<'info, T>` is the one worth noticing: it checks the owner
*and* the discriminator on every deserialize. That is the account-substitution
defence from exercise 05 turned into a type.

## Read the generated program critically

The scaffold's counter PDA is derived from **one seed**:

    seeds = [COUNTER_SEED]     // b"counter"

No user pubkey. So this is a **single global counter** for the entire program —
the first caller to run `initialize` owns it, and `increment` then rejects
everyone else via `require_keys_eq!`. That is not the same design as exercise 05,
where the seeds are `[b"counter", user]` and every user gets their own.

Neither is wrong; they answer different questions. But the scaffold reads like a
per-user counter and is not one, which is worth catching yourself rather than
copying forward.

Also note `handle_initialize` transfers `HELLO_WORLD_LAMPORTS` (1 lamport) to the
counter after creating it — decoration, not mechanism.

## Note

`Anchor.toml` points `wallet` at `~/.config/solana/id.json`, which does not exist
on this machine. It is not needed here: `skip_local_validator = true` and the
tests run in litesvm, in-process. It *would* be needed to deploy to localnet.
