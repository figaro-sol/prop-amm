#![no_std]

pub mod error;
pub mod instructions;
pub mod math;
pub mod pda;
pub mod state;
pub mod token;

use instructions::{swap::process_swap, withdraw::process_withdraw, Instruction};
use pinocchio::{error::ProgramError, program_entrypoint, AccountView, Address, ProgramResult};

program_entrypoint!(process_instruction);
pinocchio::default_allocator!();
pinocchio::nostd_panic_handler!();

/// Program entrypoint
fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let instruction = Instruction::try_from_u8(instruction_data[0])
        .ok_or(ProgramError::InvalidInstructionData)?;

    let data = &instruction_data[1..];

    match instruction {
        Instruction::Swap => process_swap(program_id, accounts, data),
        Instruction::Withdraw => process_withdraw(program_id, accounts, data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_discriminators() {
        assert_eq!(Instruction::try_from_u8(0), Some(Instruction::Swap));
        assert_eq!(Instruction::try_from_u8(1), Some(Instruction::Withdraw));
        assert_eq!(Instruction::try_from_u8(2), None);
    }
}
