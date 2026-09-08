# 06 — Standard SPL Token

Create a mint, mint supply to an ATA, transfer between two wallets — decoding
every account at every step.

    ./run.sh                 # local validator, started and torn down
    cargo run -- <RPC URL>

## The two layouts

Both are **fixed-size and positional**. No discriminator, no length prefix, no
borsh — just bytes at known offsets.

**Mint, 82 bytes**

    [0..36]   mint_authority    COption<Pubkey>
    [36..44]  supply            u64
    [44]      decimals          u8
    [45]      is_initialized    bool
    [46..82]  freeze_authority  COption<Pubkey>

**Account, 165 bytes**

    [0..32]    mint              Pubkey
    [32..64]   owner             Pubkey
    [64..72]   amount            u64
    [72..108]  delegate          COption<Pubkey>
    [108]      state             u8   0=Uninitialized 1=Initialized 2=Frozen
    [109..121] is_native         COption<u64>
    [121..129] delegated_amount  u64
    [129..165] close_authority   COption<Pubkey>

### The COption trap

`COption` is encoded **differently in account data than in instruction data**:

| | tag width |
|---|---|
| account data (what you decode) | **4 bytes**, little-endian |
| instruction data (what you send) | **1 byte** |

Same concept, two wire formats. Mixing them up doesn't error — it silently reads
or writes the wrong offsets. This exercise implements both, side by side, so the
difference is visible.

## What each step shows

1. **A wallet holds no token information.** Zero data, owned by the System
   Program. Everything else hangs off it by *derivation*, never by a field
   stored in it.
2. **A mint is a supply counter plus two authorities.** Freshly created, supply
   is 0. It holds no record of who owns what.
3. **A token account has two owners, and this is where the word bites.** The
   runtime `owner` is the Token program. The `owner` field at `[32..64]` is
   alice. Same word, printed one above the other, meaning different things.
4. **Minting is creation, not a transfer.** Two accounts change: the mint's
   supply counter and the token account's amount. Nothing is debited from
   anywhere, and only `mint_authority` can do it.
5. **A derivable address is not an account.** Transferring to bob's ATA before it
   exists fails with `InvalidAccountData`. The address computes fine — nothing
   is there.
6. **Receiving needs no signature.** Alice creates bob's ATA, pays its rent, and
   transfers into it. Bob never signs anything and never appears as a signer.
   Only *sending* requires a signature.
7. **The invariant.** `alice.amount + bob.amount == mint.supply`. The mint counts,
   the token accounts hold, and nothing reconciles the two for you — the Token
   program's arithmetic is the only thing keeping them in step.

## Instruction encodings used

    InitializeMint2 = 20 ++ decimals:u8 ++ authority:[32] ++ COption<Pubkey>(1-byte tag)
    MintTo          =  7 ++ amount:u64
    TransferChecked = 12 ++ amount:u64 ++ decimals:u8

`TransferChecked` over `Transfer` (tag 3): it takes the mint and the expected
decimals, so a client that has the wrong idea about a token's precision fails
loudly instead of moving the wrong amount.
