# Bootcamp Solana

Hands-on exercises for understanding the Solana account model from the CLI.

Each exercise is a single self-contained script. Nothing here reads or writes your
global `~/.config/solana/cli/config.yml`, and nothing needs a funded wallet.

| # | Exercise | What it teaches |
|---|----------|-----------------|
| [01](01-hello-account/) | Hello Account | An address is not an account. Rent-exemption is the price of existing. The five fields. |
| [02](02-account-inspector/) | Account Inspector | Decode a real on-chain account and verify the rent math against the cluster's own sysvars. |
| [03](03-four-programs/) | Four Pre-Built Programs | System, SPL Token, Token-2022 and the ATA program, driven from Rust with hand-encoded instructions. |

## Requirements

    solana --version     # Agave CLI, tested on 4.0.2
    spl-token --version
    jq --version
    python3 --version
    cargo --version      # exercise 03 only, tested on 1.96.0

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
