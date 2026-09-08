//! A per-user counter stored in a PDA.
//!
//! A PDA has no private key. It is an address derived from seeds that falls off
//! the ed25519 curve, so nobody can sign for it — except the program it was
//! derived under, which signs by passing the seeds to `invoke_signed`. That is
//! the whole mechanism: the runtime re-derives the address from the seeds you
//! present and, if it matches an account your program owns, treats it as signed.

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
// system_instruction and the System Program id moved out of solana-program in v4.
use solana_sdk_ids::system_program;
use solana_system_interface::instruction::create_account;

/// count: u64 | authority: [u8; 32] | bump: u8
pub const COUNTER_LEN: usize = 8 + 32 + 1;
pub const SEED_PREFIX: &[u8] = b"counter";

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let (tag, rest) = data.split_first().ok_or(ProgramError::InvalidInstructionData)?;
    match tag {
        0 => initialize(program_id, accounts),
        1 => {
            let by = u64::from_le_bytes(
                rest.get(..8).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap(),
            );
            increment(program_id, accounts, by)
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

/// Create the PDA. The program signs for an address it does not hold a key to.
fn initialize(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let iter = &mut accounts.iter();
    let user = next_account_info(iter)?;
    let counter = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    if !user.is_signer {
        msg!("user must sign");
        return Err(ProgramError::MissingRequiredSignature);
    }
    if system.key != &system_program::ID {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Re-derive rather than trust. The caller chose which account to pass in; only
    // this check ties it to this user.
    let (expected, bump) =
        Pubkey::find_program_address(&[SEED_PREFIX, user.key.as_ref()], program_id);
    if expected != *counter.key {
        msg!("counter account is not the PDA for this user");
        return Err(ProgramError::InvalidSeeds);
    }
    if !counter.data_is_empty() {
        msg!("counter already initialized");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let lamports = Rent::get()?.minimum_balance(COUNTER_LEN);

    // The PDA is not a signer on the incoming transaction — it cannot be, there is
    // no key. invoke_signed supplies the seeds, the runtime re-derives the address,
    // and the CPI is executed as though the PDA had signed.
    invoke_signed(
        &create_account(
            user.key,
            counter.key,
            lamports,
            COUNTER_LEN as u64,
            program_id,
        ),
        &[user.clone(), counter.clone(), system.clone()],
        &[&[SEED_PREFIX, user.key.as_ref(), &[bump]]],
    )?;

    let mut d = counter.try_borrow_mut_data()?;
    d[0..8].copy_from_slice(&0u64.to_le_bytes());
    d[8..40].copy_from_slice(user.key.as_ref());
    d[40] = bump;
    msg!("initialized counter for {} (bump {})", user.key, bump);
    Ok(())
}

fn increment(program_id: &Pubkey, accounts: &[AccountInfo], by: u64) -> ProgramResult {
    let iter = &mut accounts.iter();
    let user = next_account_info(iter)?;
    let counter = next_account_info(iter)?;

    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    // Ownership check. Without it, a caller could hand us a look-alike account
    // owned by some other program and we would happily write to it.
    if counter.owner != program_id {
        msg!("counter is not owned by this program");
        return Err(ProgramError::IllegalOwner);
    }
    if counter.data_len() != COUNTER_LEN {
        return Err(ProgramError::InvalidAccountData);
    }

    let mut d = counter.try_borrow_mut_data()?;
    let stored_authority = Pubkey::try_from(&d[8..40]).map_err(|_| ProgramError::InvalidAccountData)?;
    let bump = d[40];

    // Authority check. This is the application-level rule — the runtime knows
    // nothing about it.
    if stored_authority != *user.key {
        msg!("signer is not the authority on this counter");
        return Err(ProgramError::InvalidAccountOwner);
    }
    // And confirm the account really is the PDA these seeds produce.
    let expected = Pubkey::create_program_address(
        &[SEED_PREFIX, user.key.as_ref(), &[bump]],
        program_id,
    )?;
    if expected != *counter.key {
        return Err(ProgramError::InvalidSeeds);
    }

    let count = u64::from_le_bytes(d[0..8].try_into().unwrap())
        .checked_add(by)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    d[0..8].copy_from_slice(&count.to_le_bytes());
    msg!("count = {}", count);
    Ok(())
}
