//! Send your first transaction: build it, sign it, submit it, watch it move through
//! the commitment levels — then blow the compute budget on purpose and watch a
//! failed transaction land in a block anyway.

use std::error::Error;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, Instant};

use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::{keypair::read_keypair_file, Signer},
    transaction::Transaction,
};

const SYSTEM: &str = "11111111111111111111111111111111";
const COMPUTE_BUDGET: &str = "ComputeBudget111111111111111111111111111111";
const PACKET_LIMIT: usize = 1232;

fn pk(s: &str) -> Pubkey { Pubkey::from_str(s).expect("bad pubkey") }
fn head(n: u8, s: &str) { println!("\n\x1b[1m{n}. {s}\x1b[0m\n{}", "-".repeat(64)); }
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("") }

fn explorer(url: &str, sig: &Signature) -> String {
    let base = format!("https://explorer.solana.com/tx/{sig}");
    if url.contains("devnet") { format!("{base}?cluster=devnet") }
    else if url.contains("testnet") { format!("{base}?cluster=testnet") }
    else if url.contains("mainnet") { base }
    else { format!("{base}?cluster=custom&customUrl={}", urlencode(url)) }
}

fn urlencode(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

fn sys_transfer(from: &Pubkey, to: &Pubkey, lamports: u64) -> Instruction {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&lamports.to_le_bytes());
    Instruction {
        program_id: pk(SYSTEM),
        accounts: vec![AccountMeta::new(*from, true), AccountMeta::new(*to, false)],
        data,
    }
}

/// ComputeBudget instruction 2: SetComputeUnitLimit(u32).
fn set_cu_limit(units: u32) -> Instruction {
    let mut data = vec![2u8];
    data.extend_from_slice(&units.to_le_bytes());
    Instruction { program_id: pk(COMPUTE_BUDGET), accounts: vec![], data }
}

/// ComputeBudget instruction 3: SetComputeUnitPrice(u64 micro-lamports per CU).
fn set_cu_price(micro_lamports: u64) -> Instruction {
    let mut data = vec![3u8];
    data.extend_from_slice(&micro_lamports.to_le_bytes());
    Instruction { program_id: pk(COMPUTE_BUDGET), accounts: vec![], data }
}

/// Decode the message header into the signer/writable flags it actually encodes.
/// The header is three bytes; account order is what makes them meaningful.
fn dissect(tx: &Transaction) {
    let m = &tx.message;
    let h = &m.header;
    let n = m.account_keys.len();
    let signers = h.num_required_signatures as usize;
    let ro_signed = h.num_readonly_signed_accounts as usize;
    let ro_unsigned = h.num_readonly_unsigned_accounts as usize;

    println!("  header      num_required_signatures      {signers}");
    println!("              num_readonly_signed_accounts {ro_signed}");
    println!("              num_readonly_unsigned        {ro_unsigned}");
    println!("  blockhash   {}", m.recent_blockhash);
    println!("  accounts    (order is the encoding — flags are positional)");
    for (i, key) in m.account_keys.iter().enumerate() {
        let is_signer = i < signers;
        let is_writable = if is_signer { i < signers - ro_signed } else { i < n - ro_unsigned };
        println!("    [{i}] {key}  {}{}", 
            if is_signer { "signer " } else { "       " },
            if is_writable { "writable" } else { "readonly" });
    }
    for (i, ix) in m.instructions.iter().enumerate() {
        println!("  ix[{i}]       program {} (index {})",
            m.account_keys[ix.program_id_index as usize], ix.program_id_index);
        println!("              accounts {:?}", ix.accounts);
        println!("              data     0x{}", hex(&ix.data));
    }
}

fn watch(rpc: &RpcClient, sig: &Signature, budget: Duration) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let mut last = String::new();
    while start.elapsed() < budget {
        if let Some(st) = rpc.get_signature_statuses(&[*sig])?.value[0].clone() {
            let level = st.confirmation_status.map(|c| format!("{c:?}")).unwrap_or_else(|| "?".into());
            // Print only when the COMMITMENT LEVEL changes. The confirmation count
            // ticking up in between is the cluster voting; the level is the milestone.
            if level != last {
                println!("    [{:>5.1}s] {level:<10} slot {}, confirmations {:?}",
                    start.elapsed().as_secs_f32(), st.slot, st.confirmations);
                last = level.clone();
            }
            if level == "Finalized" { return Ok(()); }
        }
        sleep(Duration::from_millis(300));
    }
    println!("    [{:>5.1}s] gave up waiting for Finalized (32 confirmations takes ~45s here)",
        start.elapsed().as_secs_f32());
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8899".into());
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    println!("cluster: {url}  (slot {})", rpc.get_slot()?);

    // On a local validator the faucet is unlimited. On devnet it is rate limited,
    // so a funded keypair can be supplied instead:  cargo run -- <URL> <KEYPAIR.json>
    let payer = match std::env::args().nth(2) {
        Some(path) => read_keypair_file(&path)
            .map_err(|e| format!("could not read keypair {path}: {e}"))?,
        None => {
            let kp = Keypair::new();
            let sig = match rpc.request_airdrop(&kp.pubkey(), 2_000_000_000) {
                Ok(sig) => sig,
                Err(e) => {
                    // main() prints Err with Debug, which escapes newlines. Print
                    // the guidance ourselves so it stays readable.
                    eprintln!("airdrop failed: {e}");
                    eprintln!();
                    eprintln!("Public faucets are rate limited. Pass a funded keypair instead:");
                    eprintln!("    cargo run -- {url} <KEYPAIR.json>");
                    std::process::exit(1);
                }
            };
            while !rpc.confirm_transaction(&sig)? {}
            kp
        }
    };
    let recipient = Pubkey::new_unique();
    println!("payer:   {}  ({} lamports)", payer.pubkey(), rpc.get_balance(&payer.pubkey())?);

    // ---------------------------------------------------------------- 1 BUILD
    head(1, "BUILD — a transaction is a message plus signatures");
    let blockhash = rpc.get_latest_blockhash()?;
    let ixs = vec![
        set_cu_limit(10_000),   // ask for less than the 200k default
        set_cu_price(1_000),    // 1000 micro-lamports per CU of priority fee
        sys_transfer(&payer.pubkey(), &recipient, 100_000_000),
    ];
    let message = Message::new_with_blockhash(&ixs, Some(&payer.pubkey()), &blockhash);
    let mut tx = Transaction::new_unsigned(message);
    dissect(&tx);
    println!("  note        the 3 instructions reference only 4 distinct accounts —");
    println!("              the message deduplicates them and instructions hold indexes.");

    // ---------------------------------------------------------------- 2 SIGN
    head(2, "SIGN — the signature covers the serialized message, nothing else");
    let msg_bytes = tx.message.serialize();
    println!("  message     {} bytes", msg_bytes.len());
    tx.sign(&[&payer], blockhash);
    let signature = tx.signatures[0];
    println!("  signature   {signature}");
    println!("  verifies    {:?}", tx.verify().is_ok());
    let wire = bincode_len(&tx);
    println!("  wire size   {wire} bytes of the {PACKET_LIMIT}-byte packet limit");
    println!("              ^ this is the real constraint on how much one tx can do.");

    // ------------------------------------------------------------ 3 SIMULATE
    head(3, "SIMULATE — always do this before you spend a fee finding out");
    let sim = rpc.simulate_transaction(&tx)?.value;
    println!("  err         {:?}", sim.err);
    println!("  CUs used    {:?} of the 10,000 we requested", sim.units_consumed);
    for l in sim.logs.iter().flatten() { println!("    | {l}"); }

    // -------------------------------------------------------------- 4 SUBMIT
    head(4, "SUBMIT — and watch it climb the commitment levels");
    let before = rpc.get_balance(&payer.pubkey())?;
    let sig = rpc.send_transaction(&tx)?;
    println!("  submitted   {sig}");
    watch(&rpc, &sig, Duration::from_secs(90))?;
    let after = rpc.get_balance(&payer.pubkey())?;
    println!("  payer paid  {} lamports (100,000,000 transferred + {} fee)",
        before - after, before - after - 100_000_000);
    println!("\n  \x1b[4m{}\x1b[0m", explorer(&url, &sig));

    // ------------------------------------------------- 5 EXCEED THE CU BUDGET
    head(5, "EXCEED THE CU BUDGET ON PURPOSE");
    let blockhash = rpc.get_latest_blockhash()?;
    let ixs = vec![
        set_cu_limit(10),   // a System transfer alone costs ~150 CU
        sys_transfer(&payer.pubkey(), &recipient, 1_000_000),
    ];
    let msg = Message::new_with_blockhash(&ixs, Some(&payer.pubkey()), &blockhash);
    let bad = Transaction::new(&[&payer], msg, blockhash);

    println!("  requested a 10 CU limit for work that needs ~150.\n");
    let sim = rpc.simulate_transaction(&bad)?.value;
    println!("  simulation err  {:?}", sim.err);
    for l in sim.logs.iter().flatten() { println!("    | {l}"); }

    println!("\n  Preflight would reject this client-side. Skipping it so the");
    println!("  transaction actually reaches a validator and fails on-chain:");
    let before = rpc.get_balance(&payer.pubkey())?;
    let sig = rpc.send_transaction_with_config(&bad, RpcSendTransactionConfig {
        skip_preflight: true,
        ..RpcSendTransactionConfig::default()
    })?;
    println!("  submitted   {sig}");
    watch(&rpc, &sig, Duration::from_secs(90))?;
    let st = rpc.get_signature_statuses(&[sig])?.value[0].clone();
    let after = rpc.get_balance(&payer.pubkey())?;
    if let Some(st) = st {
        println!("  landed in slot {}", st.slot);
        println!("  err         {:?}", st.err);
    }
    println!("  payer paid  {} lamports", before - after);
    println!("\n  The transfer did NOT happen — but the transaction is in a block and");
    println!("  the fee was still charged. Failure is a state, not an absence: the");
    println!("  runtime charges for the attempt, then reverts every account it touched.");
    println!("\n  \x1b[4m{}\x1b[0m", explorer(&url, &sig));

    Ok(())
}

fn bincode_len(tx: &Transaction) -> usize {
    // 1-byte shortvec count + 64 bytes per signature + the serialized message
    1 + tx.signatures.len() * 64 + tx.message.serialize().len()
}
