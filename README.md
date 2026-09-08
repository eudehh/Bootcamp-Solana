# Bootcamp Solana

Hands-on exercises for understanding the Solana account model from the CLI.

Each exercise is a single self-contained script. Nothing here reads or writes your
global `~/.config/solana/cli/config.yml`, and nothing needs a funded wallet.

| # | Exercise | What it teaches |
|---|----------|-----------------|
| [01](01-hello-account/) | Hello Account | An address is not an account. Rent-exemption is the price of existing. The five fields. |
| [02](02-account-inspector/) | Account Inspector | Decode a real on-chain account and verify the rent math against the cluster's own sysvars. |
| [03](03-four-programs/) | Four Pre-Built Programs | System, SPL Token, Token-2022 and the ATA program, driven from Rust with hand-encoded instructions. |
| [04](04-transaction-lifecycle/) | Transaction Lifecycle | Build, sign, simulate, submit, watch commitment levels — then exceed the CU budget on purpose. |
| [05](05-counter-pda/) | Counter Program with PDAs | An on-chain program: derive PDAs client-side, pass them in, sign for them with `invoke_signed`. |
| [06](06-spl-token/) | Standard SPL Token | Mint, ATA, transfer between wallets — every account decoded field by field at every step. |

## Requirements

    solana --version     # Agave CLI, tested on 4.0.2
    spl-token --version
    jq --version
    python3 --version
    cargo --version           # exercises 03-05, tested on 1.96.0
    cargo-build-sbf --version # exercise 05 only, tested on 4.0.0 / platform-tools v1.53

## The one thing worth taking away

An account has exactly five runtime fields: `lamports`, `data`, `owner`,
`executable`, `rent_epoch`. The runtime enforces exactly one rule about them —
**only the `owner` program may write the data or debit the lamports.** Everything
else you think of as a permission (a token account's authority, an escrow's maker,
a mint's freeze authority) is *not* a runtime field. It is a slice of the `data`
blob that some program's code chooses to check. If that program forgets the check,
nothing else in the system will catch it.

Exercise 01 shows you the five fields. Exercise 02 proves the second half by
decoding an authority straight out of the raw bytes.
