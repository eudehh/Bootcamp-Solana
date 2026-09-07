//! Four pre-built programs you'll interact with.
//!
//! Every instruction below is encoded by hand, because that is the lesson: an
//! instruction is a program id, a list of accounts, and a byte array. Nothing more.
//! The four programs differ only in which bytes they expect and what they do with
//! the accounts you hand them.
//!
//!   System Program  — hard-coded into the runtime. The only program that can create
//!                     accounts and move native SOL.
//!   SPL Token       — standard, audited. Minting, burning, transferring.
//!   Token-2022      — newer standard. Programmable fees, metadata, custom hooks.
//!   Associated      — utility. A deterministic wallet -> token account link.
//!   Token Account

use std::error::Error;
use std::str::FromStr;

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};

const SYSTEM: &str = "11111111111111111111111111111111";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
const ATA: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

const MINT_LEN: u64 = 82;
// A Token-2022 mint carrying extensions is padded to the 165-byte Account length,
// then a 1-byte account-type discriminator, then TLV entries. TransferFeeConfig is
// 108 bytes of value behind a 2-byte type and 2-byte length.
const MINT_2022_WITH_FEE_LEN: u64 = 165 + 1 + 2 + 2 + 108;

fn pk(s: &str) -> Pubkey {
    Pubkey::from_str(s).expect("bad pubkey")
}

fn head(n: u8, s: &str) {
    println!("\n\x1b[1m{}. {}\x1b[0m", n, s);
    println!("{}", "-".repeat(60));
}

fn send(
    rpc: &RpcClient,
    payer: &Keypair,
    ixs: &[Instruction],
    extra: &[&Keypair],
) -> Result<Signature, Box<dyn Error>> {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra);
    let bh = rpc.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &signers, bh);
    Ok(rpc.send_and_confirm_transaction(&tx)?)
}

fn show(rpc: &RpcClient, label: &str, key: &Pubkey) -> Result<(), Box<dyn Error>> {
    let a = rpc.get_account(key)?;
    println!("  {label}");
    println!("    address     {key}");
    println!("    lamports    {}", a.lamports);
    println!("    owner       {}  {}", a.owner, name_of(&a.owner));
    println!("    executable  {}", a.executable);
    println!("    data        {} bytes", a.data.len());
    Ok(())
}

fn name_of(p: &Pubkey) -> &'static str {
    match p.to_string().as_str() {
        SYSTEM => "(System Program)",
        TOKEN => "(SPL Token)",
        TOKEN_2022 => "(Token-2022)",
        ATA => "(Associated Token Account)",
        _ => "",
    }
}

// ---------------------------------------------------------------- System Program
// Bincode enum tags: 4 bytes, little endian.

fn sys_create_account(from: &Pubkey, new: &Pubkey, lamports: u64, space: u64, owner: &Pubkey) -> Instruction {
    let mut data = Vec::with_capacity(52);
    data.extend_from_slice(&0u32.to_le_bytes()); // CreateAccount
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(owner.as_ref());
    Instruction {
        program_id: pk(SYSTEM),
        accounts: vec![AccountMeta::new(*from, true), AccountMeta::new(*new, true)],
        data,
    }
}

fn sys_transfer(from: &Pubkey, to: &Pubkey, lamports: u64) -> Instruction {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&2u32.to_le_bytes()); // Transfer
    data.extend_from_slice(&lamports.to_le_bytes());
    Instruction {
        program_id: pk(SYSTEM),
        accounts: vec![AccountMeta::new(*from, true), AccountMeta::new(*to, false)],
        data,
    }
}

// ------------------------------------------------------- SPL Token / Token-2022
// Both share the same instruction encoding: a 1-byte tag. Token-2022 is a superset.

fn pack_option_pubkey(buf: &mut Vec<u8>, v: Option<&Pubkey>) {
    match v {
        Some(p) => {
            buf.push(1);
            buf.extend_from_slice(p.as_ref());
        }
        None => buf.push(0),
    }
}

fn initialize_mint2(program: &Pubkey, mint: &Pubkey, authority: &Pubkey, decimals: u8) -> Instruction {
    let mut data = vec![20u8]; // InitializeMint2
    data.push(decimals);
    data.extend_from_slice(authority.as_ref());
    pack_option_pubkey(&mut data, None); // no freeze authority
    Instruction { program_id: *program, accounts: vec![AccountMeta::new(*mint, false)], data }
}

fn mint_to(program: &Pubkey, mint: &Pubkey, dest: &Pubkey, authority: &Pubkey, amount: u64) -> Instruction {
    let mut data = vec![7u8]; // MintTo
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: *program,
        accounts: vec![
            AccountMeta::new(*mint, false),
            AccountMeta::new(*dest, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

fn transfer_checked(
    program: &Pubkey, src: &Pubkey, mint: &Pubkey, dst: &Pubkey, authority: &Pubkey,
    amount: u64, decimals: u8,
) -> Instruction {
    let mut data = vec![12u8]; // TransferChecked
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(decimals);
    Instruction {
        program_id: *program,
        accounts: vec![
            AccountMeta::new(*src, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*dst, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

/// Token-2022 only. Must run BEFORE InitializeMint2, on an account sized for the
/// extension. Tag 26 selects the transfer-fee extension, then 0 selects its
/// initialize sub-instruction.
fn initialize_transfer_fee_config(
    mint: &Pubkey, fee_authority: Option<&Pubkey>, withdraw_authority: Option<&Pubkey>,
    basis_points: u16, maximum_fee: u64,
) -> Instruction {
    let mut data = vec![26u8, 0u8];
    pack_option_pubkey(&mut data, fee_authority);
    pack_option_pubkey(&mut data, withdraw_authority);
    data.extend_from_slice(&basis_points.to_le_bytes());
    data.extend_from_slice(&maximum_fee.to_le_bytes());
    Instruction { program_id: pk(TOKEN_2022), accounts: vec![AccountMeta::new(*mint, false)], data }
}

// ------------------------------------------------- Associated Token Account
/// The whole point of this program: the address is *derived*, not chosen. Same
/// wallet + same mint + same token program always gives the same account, and
/// nobody has to store a mapping anywhere.
fn derive_ata(wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &pk(ATA),
    )
}

fn create_ata(funder: &Pubkey, wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Instruction {
    let (ata, _) = derive_ata(wallet, mint, token_program);
    Instruction {
        program_id: pk(ATA),
        accounts: vec![
            AccountMeta::new(*funder, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*wallet, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(pk(SYSTEM), false),
            AccountMeta::new_readonly(*token_program, false),
        ],
        data: vec![0u8], // Create
    }
}

fn token_amount(rpc: &RpcClient, ata: &Pubkey) -> Result<u64, Box<dyn Error>> {
    let d = rpc.get_account(ata)?.data;
    Ok(u64::from_le_bytes(d[64..72].try_into()?))
}

fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8899".to_string());
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    println!("cluster: {url}  (slot {})", rpc.get_slot()?);

    let payer = Keypair::new();
    let sig = rpc.request_airdrop(&payer.pubkey(), 5_000_000_000)?;
    while !rpc.confirm_transaction(&sig)? {}
    println!("payer:   {}", payer.pubkey());

    // ------------------------------------------------------------------ 1
    head(1, "System Program — the only program that can create accounts");
    let blank = Keypair::new();
    let rent = rpc.get_minimum_balance_for_rent_exemption(0)?;
    send(&rpc, &payer, &[sys_create_account(&payer.pubkey(), &blank.pubkey(), rent, 0, &pk(SYSTEM))], &[&blank])?;
    show(&rpc, "an account it created and kept for itself:", &blank.pubkey())?;
    println!("    ^ CreateAccount takes the future owner as an argument. System can");
    println!("      hand a brand-new account to any program — that is how every other");
    println!("      account in this program gets made.");

    send(&rpc, &payer, &[sys_transfer(&payer.pubkey(), &blank.pubkey(), 1_000_000)], &[])?;
    println!("\n  after a 1,000,000 lamport Transfer: {} lamports", rpc.get_account(&blank.pubkey())?.lamports);
    println!("    ^ moving native SOL is System's other job. No other program can do it.");

    // ------------------------------------------------------------------ 2
    head(2, "SPL Token — a mint is just an account System made for it");
    let mint = Keypair::new();
    let rent = rpc.get_minimum_balance_for_rent_exemption(MINT_LEN as usize)?;
    send(&rpc, &payer, &[
        sys_create_account(&payer.pubkey(), &mint.pubkey(), rent, MINT_LEN, &pk(TOKEN)),
        initialize_mint2(&pk(TOKEN), &mint.pubkey(), &payer.pubkey(), 6),
    ], &[&mint])?;
    show(&rpc, "mint:", &mint.pubkey())?;
    println!("    ^ two instructions, one transaction: System allocates and assigns,");
    println!("      then SPL Token writes its own layout into the bytes it now owns.");

    // ------------------------------------------------------------------ 3
    head(3, "Associated Token Account — the address is derived, not chosen");
    let (ata, bump) = derive_ata(&payer.pubkey(), &mint.pubkey(), &pk(TOKEN));
    let (again, _) = derive_ata(&payer.pubkey(), &mint.pubkey(), &pk(TOKEN));
    println!("  seeds       [wallet, token_program, mint]");
    println!("  derived     {ata}  (bump {bump})");
    println!("  again       {again}   <- deterministic, no lookup table anywhere");
    let (ata_2022, _) = derive_ata(&payer.pubkey(), &mint.pubkey(), &pk(TOKEN_2022));
    println!("  same wallet+mint under Token-2022 instead:");
    println!("              {ata_2022}");
    println!("    ^ different program id in the seeds => different address. The token");
    println!("      program is part of the identity of the account.");

    send(&rpc, &payer, &[create_ata(&payer.pubkey(), &payer.pubkey(), &mint.pubkey(), &pk(TOKEN))], &[])?;
    send(&rpc, &payer, &[mint_to(&pk(TOKEN), &mint.pubkey(), &ata, &payer.pubkey(), 1_000_000)], &[])?;
    println!();
    show(&rpc, "the created ATA:", &ata)?;
    println!("    balance     {} (raw, from data[64..72])", token_amount(&rpc, &ata)?);

    // ------------------------------------------------------------------ 4
    head(4, "Token-2022 — same encoding, extra behaviour");
    let mint22 = Keypair::new();
    let rent = rpc.get_minimum_balance_for_rent_exemption(MINT_2022_WITH_FEE_LEN as usize)?;
    send(&rpc, &payer, &[
        sys_create_account(&payer.pubkey(), &mint22.pubkey(), rent, MINT_2022_WITH_FEE_LEN, &pk(TOKEN_2022)),
        // extension init MUST precede InitializeMint2
        initialize_transfer_fee_config(&mint22.pubkey(), Some(&payer.pubkey()), Some(&payer.pubkey()), 500, u64::MAX),
        initialize_mint2(&pk(TOKEN_2022), &mint22.pubkey(), &payer.pubkey(), 6),
    ], &[&mint22])?;
    show(&rpc, "mint with a 5% transfer fee:", &mint22.pubkey())?;
    println!("    ^ {} bytes vs {} for a plain SPL Token mint — the extra bytes are the", MINT_2022_WITH_FEE_LEN, MINT_LEN);
    println!("      TLV entry holding TransferFeeConfig.");

    let (src, _) = derive_ata(&payer.pubkey(), &mint22.pubkey(), &pk(TOKEN_2022));
    let recipient = Keypair::new();
    let (dst, _) = derive_ata(&recipient.pubkey(), &mint22.pubkey(), &pk(TOKEN_2022));
    send(&rpc, &payer, &[
        create_ata(&payer.pubkey(), &payer.pubkey(), &mint22.pubkey(), &pk(TOKEN_2022)),
        create_ata(&payer.pubkey(), &recipient.pubkey(), &mint22.pubkey(), &pk(TOKEN_2022)),
    ], &[])?;
    send(&rpc, &payer, &[mint_to(&pk(TOKEN_2022), &mint22.pubkey(), &src, &payer.pubkey(), 1_000_000)], &[])?;
    send(&rpc, &payer, &[transfer_checked(&pk(TOKEN_2022), &src, &mint22.pubkey(), &dst, &payer.pubkey(), 100_000, 6)], &[])?;

    println!("\n  transferred 100,000 with the identical TransferChecked encoding:");
    println!("    sender   {}", token_amount(&rpc, &src)?);
    println!("    receiver {}", token_amount(&rpc, &dst)?);
    println!("    ^ the receiver is short 5,000 — 5% withheld by the mint itself.");
    println!("      The client asked for a plain transfer. The fee is the program's");
    println!("      behaviour, not the caller's. That is what Token-2022 buys you.");

    Ok(())
}
