# 03 — Four Pre-Built Programs

The four programs you interact with before you ever write one of your own.

    ./run.sh                       # starts a local validator, runs, tears down
    cargo run -- <RPC URL>         # against a validator you already have

| Program | Address | Role |
|---|---|---|
| System Program | `11111111111111111111111111111111` | Hard-coded into the runtime. The only program that can create accounts and move native SOL. |
| SPL Token | `TokenkegQfe…` | Standard, audited, on-chain. Minting, burning, transferring assets. |
| Token-2022 | `TokenzQdBNb…` | Newer standard. Programmable fees, metadata, custom hooks. |
| Associated Token Account | `ATokenGPvbd…` | Utility. A deterministic wallet → token account link. |

## Why the instructions are encoded by hand

There are no `spl-token` crates in `Cargo.toml`. `spl-token 9` is built on the
granular `solana-*` **v3** crates while `solana-sdk 4` is on **v4**, so their
`Pubkey` types do not unify and the two cannot be mixed without pinning to an
old stack.

Encoding by hand is the better lesson anyway. Interacting with a program is
nothing but:

    Instruction { program_id, accounts: Vec<AccountMeta>, data: Vec<u8> }

A program id, a list of accounts, and a byte array. That is the entire interface.
The four programs differ only in which bytes they expect.

## The encodings

**System** uses bincode, so its tags are **4 bytes little-endian**:

    CreateAccount = 0u32 ++ lamports:u64 ++ space:u64 ++ owner:[u8;32]
    Transfer      = 2u32 ++ lamports:u64

**SPL Token and Token-2022 share a 1-byte tag** — Token-2022 is a superset:

    InitializeMint2 = 20 ++ decimals:u8 ++ authority:[u8;32] ++ COption<Pubkey>
    MintTo          =  7 ++ amount:u64
    TransferChecked = 12 ++ amount:u64 ++ decimals:u8

`COption<Pubkey>` is `0` for none, or `1` followed by 32 bytes.

**Token-2022 extensions** nest behind tag 26:

    InitializeTransferFeeConfig
      = 26 ++ 0 ++ COption<Pubkey> ++ COption<Pubkey> ++ basis_points:u16 ++ max_fee:u64

It must run **before** `InitializeMint2`, on an account already sized for the
extension.

**The ATA program** takes `data: [0]` (Create). Its real value is the derivation,
which needs no instruction at all:

    find_program_address([wallet, token_program, mint], ATA_PROGRAM)

## What the run demonstrates

1. **System creates every account.** `CreateAccount` takes the *future owner* as an
   argument — that is how an account ends up belonging to SPL Token. System
   allocates and assigns; the new owner then writes its own layout into the bytes.
   Both happen in one transaction.
2. **A mint is just an account.** 82 bytes owned by `Tokenkeg…`. Nothing magic.
3. **The ATA address is derived, not chosen.** Derive it twice, get the same answer,
   with no lookup table anywhere. And because the token program id is one of the
   seeds, the same wallet and same mint under Token-2022 resolve to a *different*
   address — the token program is part of the account's identity.
4. **Token-2022 changes behaviour, not encoding.** The mint is created with a 5%
   transfer fee. The client then sends the byte-identical `TransferChecked` used
   for SPL Token, and the receiver lands 95,000 of 100,000. The fee is the
   program's behaviour, not the caller's — a client that knows nothing about fees
   still pays them.

A fee-bearing mint is 278 bytes against 82 for a plain one: padded to the 165-byte
account length, a 1-byte type discriminator, then a TLV entry of 2-byte type,
2-byte length, and 108 bytes of `TransferFeeConfig`.
