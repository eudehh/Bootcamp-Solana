//! Standard SPL Token, end to end, decoding every account at every step.
//!
//! Two layouts matter. Both are fixed-size and positional — no discriminator, no
//! length prefix, no borsh:
//!
//!   Mint    82 bytes
//!   Account 165 bytes
//!
//! Note the COption encoding differs between *account data* and *instruction data*.
//! In account data the tag is 4 bytes. In instruction data it is 1. Same concept,
//! different wire format, and mixing them up produces silent nonsense.

use std::error::Error;
use std::str::FromStr;

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};

const SYSTEM: &str = "11111111111111111111111111111111";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ATA: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const MINT_LEN: u64 = 82;
const DECIMALS: u8 = 6;

fn pk(s: &str) -> Pubkey { Pubkey::from_str(s).expect("bad pubkey") }
fn head(n: u8, s: &str) { println!("\n\x1b[1m{n}. {s}\x1b[0m\n{}", "-".repeat(66)); }
fn ui(raw: u64) -> String { format!("{}.{:06}", raw / 1_000_000, raw % 1_000_000) }

// ---------------------------------------------------------------- decoders
/// COption in ACCOUNT data: 4-byte little-endian tag, then the value.
fn coption_pubkey(d: &[u8]) -> Option<Pubkey> {
    match u32::from_le_bytes(d[0..4].try_into().unwrap()) {
        0 => None,
        _ => Some(Pubkey::try_from(&d[4..36]).unwrap()),
    }
}
fn coption_u64(d: &[u8]) -> Option<u64> {
    match u32::from_le_bytes(d[0..4].try_into().unwrap()) {
        0 => None,
        _ => Some(u64::from_le_bytes(d[4..12].try_into().unwrap())),
    }
}

fn show_runtime(rpc: &RpcClient, key: &Pubkey) -> Result<Vec<u8>, Box<dyn Error>> {
    let a = rpc.get_account(key)?;
    println!("    address           {key}");
    println!("    lamports          {}  (rent-exempt reserve)", a.lamports);
    println!("    owner             {}", a.owner);
    println!("    executable        {}", a.executable);
    println!("    data              {} bytes", a.data.len());
    Ok(a.data)
}

fn show_mint(rpc: &RpcClient, key: &Pubkey) -> Result<(), Box<dyn Error>> {
    println!("  \x1b[1mMINT\x1b[0m");
    let d = show_runtime(rpc, key)?;
    println!("    -- decoded (82-byte Mint layout) --");
    println!("    [0..36]   mint_authority    {:?}", coption_pubkey(&d[0..36]));
    println!("    [36..44]  supply            {} ({} UI)",
        u64::from_le_bytes(d[36..44].try_into()?), ui(u64::from_le_bytes(d[36..44].try_into()?)));
    println!("    [44]      decimals          {}", d[44]);
    println!("    [45]      is_initialized    {}", d[45] != 0);
    println!("    [46..82]  freeze_authority  {:?}", coption_pubkey(&d[46..82]));
    Ok(())
}

fn show_token_account(rpc: &RpcClient, label: &str, key: &Pubkey) -> Result<(), Box<dyn Error>> {
    println!("  \x1b[1mTOKEN ACCOUNT\x1b[0m — {label}");
    let d = show_runtime(rpc, key)?;
    println!("    -- decoded (165-byte Account layout) --");
    println!("    [0..32]    mint              {}", Pubkey::try_from(&d[0..32])?);
    println!("    [32..64]   owner             {}", Pubkey::try_from(&d[32..64])?);
    let amount = u64::from_le_bytes(d[64..72].try_into()?);
    println!("    [64..72]   amount            {amount} ({} UI)", ui(amount));
    println!("    [72..108]  delegate          {:?}", coption_pubkey(&d[72..108]));
    println!("    [108]      state             {}", match d[108] {
        0 => "Uninitialized", 1 => "Initialized", 2 => "Frozen", _ => "?" });
    println!("    [109..121] is_native         {:?}", coption_u64(&d[109..121]));
    println!("    [121..129] delegated_amount  {}", u64::from_le_bytes(d[121..129].try_into()?));
    println!("    [129..165] close_authority   {:?}", coption_pubkey(&d[129..165]));
    Ok(())
}

// ------------------------------------------------------------ instructions
// Token instruction data uses a 1-BYTE COption tag. Account data uses 4. Careful.
fn coption_ix(buf: &mut Vec<u8>, v: Option<&Pubkey>) {
    match v { Some(p) => { buf.push(1); buf.extend_from_slice(p.as_ref()); } None => buf.push(0) }
}

fn sys_create_account(from: &Pubkey, new: &Pubkey, lamports: u64, space: u64, owner: &Pubkey) -> Instruction {
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(owner.as_ref());
    Instruction { program_id: pk(SYSTEM),
        accounts: vec![AccountMeta::new(*from, true), AccountMeta::new(*new, true)], data }
}

fn initialize_mint2(mint: &Pubkey, authority: &Pubkey, freeze: Option<&Pubkey>) -> Instruction {
    let mut data = vec![20u8, DECIMALS];
    data.extend_from_slice(authority.as_ref());
    coption_ix(&mut data, freeze);
    Instruction { program_id: pk(TOKEN), accounts: vec![AccountMeta::new(*mint, false)], data }
}

fn mint_to(mint: &Pubkey, dest: &Pubkey, authority: &Pubkey, amount: u64) -> Instruction {
    let mut data = vec![7u8];
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction { program_id: pk(TOKEN), accounts: vec![
        AccountMeta::new(*mint, false),
        AccountMeta::new(*dest, false),
        AccountMeta::new_readonly(*authority, true)], data }
}

fn transfer_checked(src: &Pubkey, mint: &Pubkey, dst: &Pubkey, authority: &Pubkey, amount: u64) -> Instruction {
    let mut data = vec![12u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(DECIMALS);
    Instruction { program_id: pk(TOKEN), accounts: vec![
        AccountMeta::new(*src, false),
        AccountMeta::new_readonly(*mint, false),
        AccountMeta::new(*dst, false),
        AccountMeta::new_readonly(*authority, true)], data }
}

fn derive_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[wallet.as_ref(), pk(TOKEN).as_ref(), mint.as_ref()], &pk(ATA)).0
}

fn create_ata(funder: &Pubkey, wallet: &Pubkey, mint: &Pubkey) -> Instruction {
    Instruction { program_id: pk(ATA), accounts: vec![
        AccountMeta::new(*funder, true),
        AccountMeta::new(derive_ata(wallet, mint), false),
        AccountMeta::new_readonly(*wallet, false),
        AccountMeta::new_readonly(*mint, false),
        AccountMeta::new_readonly(pk(SYSTEM), false),
        AccountMeta::new_readonly(pk(TOKEN), false)], data: vec![0] }
}

fn send(rpc: &RpcClient, payer: &Keypair, ixs: &[Instruction], extra: &[&Keypair]) -> Result<(), Box<dyn Error>> {
    let mut signers = vec![payer];
    signers.extend_from_slice(extra);
    let bh = rpc.get_latest_blockhash()?;
    rpc.send_and_confirm_transaction(&Transaction::new_signed_with_payer(
        ixs, Some(&payer.pubkey()), &signers, bh))?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8899".into());
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    println!("cluster: {url}");

    let alice = Keypair::new();
    let bob = Keypair::new();
    let sig = rpc.request_airdrop(&alice.pubkey(), 5_000_000_000)?;
    while !rpc.confirm_transaction(&sig)? {}
    println!("alice (wallet): {}", alice.pubkey());
    println!("bob   (wallet): {}", bob.pubkey());

    // ------------------------------------------------------------------ 1
    head(1, "THE WALLETS — System-owned, zero data");
    show_runtime(&rpc, &alice.pubkey())?;
    println!("    ^ a wallet holds no token information whatsoever. It is an empty");
    println!("      account owned by the System Program. Everything below hangs off it");
    println!("      by derivation, never by a field stored here.");

    // ------------------------------------------------------------------ 2
    head(2, "CREATE THE MINT");
    let mint = Keypair::new();
    let rent = rpc.get_minimum_balance_for_rent_exemption(MINT_LEN as usize)?;
    send(&rpc, &alice, &[
        sys_create_account(&alice.pubkey(), &mint.pubkey(), rent, MINT_LEN, &pk(TOKEN)),
        initialize_mint2(&mint.pubkey(), &alice.pubkey(), None),
    ], &[&mint])?;
    show_mint(&rpc, &mint.pubkey())?;
    println!("    ^ supply is 0. A mint is a supply counter plus two authorities —");
    println!("      it holds no list of who owns what.");

    // ------------------------------------------------------------------ 3
    head(3, "CREATE ALICE'S ATA");
    let alice_ata = derive_ata(&alice.pubkey(), &mint.pubkey());
    println!("  derived from [wallet, token_program, mint] -> {alice_ata}\n");
    send(&rpc, &alice, &[create_ata(&alice.pubkey(), &alice.pubkey(), &mint.pubkey())], &[])?;
    show_token_account(&rpc, "alice", &alice_ata)?;
    println!("    ^ the token account's `owner` field (alice) is NOT the runtime owner");
    println!("      shown above it (the Token program). Two different meanings, one word.");

    // ------------------------------------------------------------------ 4
    head(4, "MINT SUPPLY TO ALICE");
    send(&rpc, &alice, &[mint_to(&mint.pubkey(), &alice_ata, &alice.pubkey(), 1_000_000_000)], &[])?;
    show_mint(&rpc, &mint.pubkey())?;
    println!();
    show_token_account(&rpc, "alice", &alice_ata)?;
    println!("    ^ two accounts changed. The mint's supply counter went up, and the");
    println!("      token account's amount went up. Minting is not a transfer from");
    println!("      anywhere — it is creation, and only mint_authority can do it.");

    // ------------------------------------------------------------------ 5
    head(5, "TRANSFER BEFORE BOB HAS AN ATA");
    let bob_ata = derive_ata(&bob.pubkey(), &mint.pubkey());
    println!("  bob's ATA would be {bob_ata}");
    println!("  the address is derivable, but nothing exists there yet.\n");
    let bh = rpc.get_latest_blockhash()?;
    let doomed = Transaction::new_signed_with_payer(
        &[transfer_checked(&alice_ata, &mint.pubkey(), &bob_ata, &alice.pubkey(), 1)],
        Some(&alice.pubkey()), &[&alice], bh);
    println!("  err  {:?}", rpc.simulate_transaction(&doomed)?.value.err);
    println!("    ^ a derivable address is not an account. Someone has to pay rent to");
    println!("      bring it into existence first — and it need not be bob.");

    // ------------------------------------------------------------------ 6
    head(6, "CREATE BOB'S ATA (paid for by alice) AND TRANSFER");
    send(&rpc, &alice, &[
        create_ata(&alice.pubkey(), &bob.pubkey(), &mint.pubkey()),
        transfer_checked(&alice_ata, &mint.pubkey(), &bob_ata, &alice.pubkey(), 250_000_000),
    ], &[])?;
    show_token_account(&rpc, "alice", &alice_ata)?;
    println!();
    show_token_account(&rpc, "bob", &bob_ata)?;
    println!("    ^ bob never signed. Receiving needs no signature — only sending does.");
    println!("      Alice paid the rent for an account whose `owner` field is bob.");

    // ------------------------------------------------------------------ 7
    head(7, "THE INVARIANT");
    let d = rpc.get_account(&mint.pubkey())?.data;
    let supply = u64::from_le_bytes(d[36..44].try_into()?);
    let a = u64::from_le_bytes(rpc.get_account(&alice_ata)?.data[64..72].try_into()?);
    let b = u64::from_le_bytes(rpc.get_account(&bob_ata)?.data[64..72].try_into()?);
    println!("  mint.supply        {supply}");
    println!("  alice.amount       {a}");
    println!("  bob.amount         {b}");
    println!("  alice + bob        {}   balanced: {}", a + b, a + b == supply);
    println!("    ^ the mint counts, the token accounts hold. Nothing reconciles these");
    println!("      for you — the Token program's arithmetic is the only thing keeping");
    println!("      them in step.");
    Ok(())
}
