# 05 — Counter Program with PDAs

A per-user counter stored in a Program Derived Address. The client derives the
PDA, passes it into the instruction, and the program signs for it with
`invoke_signed`.

    ./run.sh    # builds the program, boots a validator with it preloaded, runs the client

    program/    the on-chain program (cargo-build-sbf -> counter_program.so)
    client/     derives PDAs and drives it

## What a PDA actually is

An address derived from seeds that **falls off the ed25519 curve**, so no private
key exists for it. Nobody can sign for it — not even whoever derived it:

    seeds        [b"counter", user_pubkey]
    alice        -> EJzkXv6m…   bump 255   on_curve = false
    bob          -> HWndMUiK…   bump 253   on_curve = false

`find_program_address` counts down from 255 appending a **bump** byte until the
resulting hash lands off the curve. Alice's needed no nudge, Bob's took three —
which is why you store the bump rather than re-deriving blindly.

## How the program signs for it

The PDA is passed into the instruction with `is_signer = false`. It cannot be a
signer on a transaction; there is no key to sign with.

    AccountMeta::new(pda, false)     // <- false. Always.

The program then calls System's `create_account` through `invoke_signed`, handing
the runtime the seeds:

    invoke_signed(
        &create_account(user.key, counter.key, lamports, COUNTER_LEN, program_id),
        &[user, counter, system],
        &[&[b"counter", user.key.as_ref(), &[bump]]],
    )

The runtime re-derives the address from those seeds under the calling program's
id. If it matches, the PDA counts as a signer **for that CPI**. That is the entire
mechanism — a program can sign for any address derivable from its own id, and for
no others.

## Layout

    count: u64 | authority: [u8; 32] | bump: u8      = 41 bytes

## The part worth internalising

Section 4 has Alice sign a validly formed transaction that passes **Bob's** PDA:

    err  InstructionError(0, InvalidAccountOwner)
      | Program log: signer is not the authority on this counter
      | consumed 753 of 200000 compute units

Nothing in the runtime objected. The account was writable, it was owned by the
program, and the transaction carried a real signature from a real keypair. The
runtime's only rule — *only the owner program may write these bytes* — was
satisfied, because the owner program is exactly what was writing them.

It failed for one reason: the program compared the authority stored in the
account's data against the signer, and refused. Delete those four lines and Alice
increments Bob's counter.

That is the split from exercise 01 seen from the other side. `owner` is enforced
by the runtime. `authority` is enforced by you, or by nobody.

The program therefore re-derives rather than trusts, in **both** instructions:

- `initialize` recomputes `find_program_address` and rejects a mismatch
  (`InvalidSeeds`) — the caller chose which account to pass in.
- `increment` checks `counter.owner == program_id` (a look-alike account owned by
  another program would otherwise be written to), that the length matches, that
  the stored authority is the signer, and that the account really is the PDA those
  seeds produce via `create_program_address`.

## Note on solana-program 4

`system_instruction` and the System Program id are no longer in `solana-program`
itself. They now come from separate crates:

    solana-system-interface = { version = "3", features = ["bincode"] }
    solana-sdk-ids = "3"
