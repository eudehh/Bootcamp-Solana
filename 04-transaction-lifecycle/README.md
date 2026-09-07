# 04 — Transaction Lifecycle

Build a transaction, sign it, submit it, watch it climb the commitment levels —
then blow the compute budget on purpose.

    ./run.sh                                  # local validator, started and torn down
    cargo run -- <RPC URL>                    # a cluster you already have
    cargo run -- <RPC URL> <KEYPAIR.json>     # devnet, where the faucet is rate limited

## 1. Build — a transaction is a message plus signatures

The message is dissected field by field. The three-byte header is the whole
permission model of a transaction, and it is **positional** — it encodes counts,
not flags, and the account order is what makes them meaningful:

    num_required_signatures       1
    num_readonly_signed_accounts  0
    num_readonly_unsigned         2

    [0] 72eLQ7fL…   signer writable     <- fee payer, always index 0
    [1] 11157t3s…          writable
    [2] 1111…1111          readonly     <- System Program
    [3] ComputeBudget…     readonly

Account `i` is a signer if `i < num_required_signatures`, and writable by the two
count boundaries. There is no per-account flag byte anywhere.

Three instructions reference only **four distinct accounts**: the message
deduplicates them and each instruction holds *indexes* into that array.

    ix[0]  ComputeBudget  data 0x0210270000              SetComputeUnitLimit(10_000)
    ix[1]  ComputeBudget  data 0x03e803000000000000      SetComputeUnitPrice(1_000)
    ix[2]  System         data 0x0200000000e1f50500000000 Transfer(100_000_000)

## 2. Sign

The signature covers the **serialized message and nothing else**. Message: 202
bytes. Wire size: 267 bytes, against the **1232-byte packet limit** — that limit,
not gas, is the real ceiling on how much one transaction can do.

## 3. Simulate

`simulateTransaction` before you spend a fee finding out. It returns the program
logs and `units_consumed`: **450 CU** actually used of the 10,000 requested.

## 4. Submit, and watch the commitment levels

    [  0.9s] Confirmed  slot 4, confirmations Some(1)
    [ 44.1s] Finalized  slot 4, confirmations None

`Processed` → `Confirmed` → `Finalized` are the milestones; the confirmation
count ticking up in between is the cluster voting. Finalization is ~32
confirmations, which is why it takes ~45s here and why almost everything in
practice waits for `Confirmed` instead.

Fee paid: **5010 lamports** — 5000 base, plus 10 of priority fee
(10,000 CU × 1000 micro-lamports = 10,000,000 micro-lamports = 10 lamports).

## 5. Exceed the CU budget on purpose

Ask for a 10 CU limit for work that needs ~150:

    simulation err  Some(InstructionError(0, ComputationalBudgetExceeded))
      | Program ComputeBudget… failed: Computational budget exceeded

Preflight would reject this client-side, so the exercise resubmits with
`skip_preflight: true` to make it fail **on-chain** instead:

    landed in slot 37
    err         Some(InstructionError(0, ComputationalBudgetExceeded))
    payer paid  5000 lamports

**The transfer did not happen, but the transaction is in a block and the fee was
still charged.** Failure is a state, not an absence: the runtime charges for the
attempt, then reverts every account the transaction touched. This is the single
most useful thing to internalise about the lifecycle — "it failed" and "it never
happened" are different claims, and only the second one is free.

## Explorer

Each submitted signature prints an explorer URL. Against a local validator it uses
the custom-cluster form, which your browser resolves against your own machine:

    https://explorer.solana.com/tx/<sig>?cluster=custom&customUrl=http%3A%2F%2F127.0.0.1%3A8899

Point the exercise at devnet and it emits `?cluster=devnet` links instead.

## Compute budget encoding

`ComputeBudget111111111111111111111111111111` takes a 1-byte tag:

    SetComputeUnitLimit = 2 ++ units:u32
    SetComputeUnitPrice = 3 ++ micro_lamports_per_cu:u64

The default without them is 200,000 CU per instruction, 1.4M per transaction.
