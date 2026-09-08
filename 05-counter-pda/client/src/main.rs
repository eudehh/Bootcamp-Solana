//! Drive the counter program: derive PDAs client-side, pass them in, and watch the
//! program sign for an address nobody holds a key to.

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
const SEED_PREFIX: &[u8] = b"counter";

fn head(n: u8, s: &str) { println!("\n\x1b[1m{n}. {s}\x1b[0m\n{}", "-".repeat(64)); }

/// The client derives the same address the program will re-derive. Neither side
/// stores it — the seeds *are* the address.
fn counter_pda(user: &Pubkey, program: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SEED_PREFIX, user.as_ref()], program)
}

fn ix_initialize(program: &Pubkey, user: &Pubkey) -> Instruction {
    let (pda, _) = counter_pda(user, program);
    Instruction {
        program_id: *program,
        accounts: vec![
            AccountMeta::new(*user, true),
            AccountMeta::new(pda, false), // NOT a signer — it cannot be
            AccountMeta::new_readonly(Pubkey::from_str(SYSTEM).unwrap(), false),
        ],
        data: vec![0],
    }
}

fn ix_increment(program: &Pubkey, user: &Pubkey, pda: &Pubkey, by: u64) -> Instruction {
    let mut data = vec![1u8];
    data.extend_from_slice(&by.to_le_bytes());
    Instruction {
        program_id: *program,
        accounts: vec![AccountMeta::new(*user, true), AccountMeta::new(*pda, false)],
        data,
    }
}

fn send(rpc: &RpcClient, payer: &Keypair, ixs: &[Instruction]) -> Result<(), Box<dyn Error>> {
    let bh = rpc.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &[payer], bh);
    rpc.send_and_confirm_transaction(&tx)?;
    Ok(())
}

fn read_counter(rpc: &RpcClient, pda: &Pubkey) -> Result<(u64, Pubkey, u8), Box<dyn Error>> {
    let d = rpc.get_account(pda)?.data;
    Ok((
        u64::from_le_bytes(d[0..8].try_into()?),
        Pubkey::try_from(&d[8..40])?,
        d[40],
    ))
}

fn fund(rpc: &RpcClient) -> Result<Keypair, Box<dyn Error>> {
    let kp = Keypair::new();
    let sig = rpc.request_airdrop(&kp.pubkey(), 2_000_000_000)?;
    while !rpc.confirm_transaction(&sig)? {}
    Ok(kp)
}

fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8899".into());
    let program: Pubkey = std::env::args().nth(2)
        .ok_or("usage: counter-client <RPC_URL> <PROGRAM_ID>")?
        .parse()?;
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    println!("cluster: {url}\nprogram: {program}");

    let alice = fund(&rpc)?;
    let bob = fund(&rpc)?;

    // ---------------------------------------------------------------- 1
    head(1, "DERIVE — the address is a function of the seeds");
    let (alice_pda, alice_bump) = counter_pda(&alice.pubkey(), &program);
    let (bob_pda, bob_bump) = counter_pda(&bob.pubkey(), &program);
    println!("  seeds        [b\"counter\", user_pubkey]");
    println!("  alice  {}  bump {alice_bump}", alice.pubkey());
    println!("    -> pda     {alice_pda}");
    println!("  bob    {}  bump {bob_bump}", bob.pubkey());
    println!("    -> pda     {bob_pda}");
    println!("  on the ed25519 curve? alice_pda={} bob_pda={}",
        alice_pda.is_on_curve(), bob_pda.is_on_curve());
    println!("    ^ off the curve means no private key exists for it. Nobody can");
    println!("      sign for these addresses — not even whoever derived them.");
    println!("  the bump is the nudge that pushed the hash off the curve; find_program_address");
    println!("  counts down from 255 and returns the first seed set that lands off it.");

    // ---------------------------------------------------------------- 2
    head(2, "INITIALIZE — the program signs via invoke_signed");
    println!("  the PDA is passed with is_signer = false. It cannot sign a transaction.");
    send(&rpc, &alice, &[ix_initialize(&program, &alice.pubkey())])?;
    let acct = rpc.get_account(&alice_pda)?;
    println!("  created     {alice_pda}");
    println!("    lamports  {}", acct.lamports);
    println!("    owner     {}", acct.owner);
    println!("    data      {} bytes", acct.data.len());
    let (count, authority, bump) = read_counter(&rpc, &alice_pda)?;
    println!("    decoded   count={count} authority={authority} bump={bump}");
    println!("    ^ System created it, but the *program* authorized the CPI by handing");
    println!("      the runtime [b\"counter\", alice, bump]. The runtime re-derived the");
    println!("      address, matched it, and treated the PDA as a signer.");

    // ---------------------------------------------------------------- 3
    head(3, "INCREMENT — per-user state, no collisions");
    send(&rpc, &bob, &[ix_initialize(&program, &bob.pubkey())])?;
    send(&rpc, &alice, &[
        ix_increment(&program, &alice.pubkey(), &alice_pda, 1),
        ix_increment(&program, &alice.pubkey(), &alice_pda, 41),
    ])?;
    send(&rpc, &bob, &[ix_increment(&program, &bob.pubkey(), &bob_pda, 7)])?;
    println!("  alice count {}", read_counter(&rpc, &alice_pda)?.0);
    println!("  bob   count {}", read_counter(&rpc, &bob_pda)?.0);
    println!("    ^ two instructions in one transaction both hit alice's counter:");
    println!("      1 + 41 = 42. Bob's is a different account entirely.");

    // ---------------------------------------------------------------- 4
    head(4, "THE CHECK THAT MATTERS — alice signs, but passes bob's PDA");
    let bad = Transaction::new_signed_with_payer(
        &[ix_increment(&program, &alice.pubkey(), &bob_pda, 1_000_000)],
        Some(&alice.pubkey()),
        &[&alice],
        rpc.get_latest_blockhash()?,
    );
    let sim = rpc.simulate_transaction(&bad)?.value;
    println!("  err  {:?}", sim.err);
    for l in sim.logs.iter().flatten() { println!("    | {l}"); }
    println!("  bob count still {}", read_counter(&rpc, &bob_pda)?.0);
    println!("    ^ nothing in the runtime stopped this. The account was writable and");
    println!("      owned by the program; the transaction was validly signed by alice.");
    println!("      It failed only because the program compared the authority stored");
    println!("      in the data against the signer. Delete that check and the transfer");
    println!("      of someone else's counter goes through.");

    Ok(())
}
