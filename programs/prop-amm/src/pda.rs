//! PDA (Program Derived Address) Derivation
//!
//! Defines seeds and derivation functions for program-owned accounts.

use pinocchio::pubkey::{create_program_address, Pubkey};

/// Seed prefix for Pool PDA
pub const POOL_SEED: &[u8] = b"pool";

/// Seed prefix for base token vault PDA
pub const BASE_VAULT_SEED: &[u8] = b"base_vault";

/// Seed prefix for quote token vault PDA
pub const QUOTE_VAULT_SEED: &[u8] = b"quote_vault";

/// Create Pool PDA with known bump
///
/// Seeds: ["pool", base_mint, quote_mint, bump]
pub fn create_pool_address_with_bump(
    program_id: &Pubkey,
    base_mint: &Pubkey,
    quote_mint: &Pubkey,
    bump: u8,
) -> Option<Pubkey> {
    create_program_address(
        &[POOL_SEED, base_mint.as_ref(), quote_mint.as_ref(), &[bump]],
        program_id,
    )
    .ok()
}

/// Create base vault PDA with known bump
///
/// Seeds: ["base_vault", pool, bump]
pub fn create_base_vault_address_with_bump(
    program_id: &Pubkey,
    pool: &Pubkey,
    bump: u8,
) -> Option<Pubkey> {
    create_program_address(&[BASE_VAULT_SEED, pool.as_ref(), &[bump]], program_id).ok()
}

/// Create quote vault PDA with known bump
///
/// Seeds: ["quote_vault", pool, bump]
pub fn create_quote_vault_address_with_bump(
    program_id: &Pubkey,
    pool: &Pubkey,
    bump: u8,
) -> Option<Pubkey> {
    create_program_address(&[QUOTE_VAULT_SEED, pool.as_ref(), &[bump]], program_id).ok()
}

/// Get seeds for Pool PDA signing
#[inline]
pub fn pool_seeds<'a>(base_mint: &'a [u8], quote_mint: &'a [u8], bump: &'a [u8]) -> [&'a [u8]; 4] {
    [POOL_SEED, base_mint, quote_mint, bump]
}

/// Get seeds for base vault PDA signing
#[inline]
pub fn base_vault_seeds<'a>(pool: &'a [u8], bump: &'a [u8]) -> [&'a [u8]; 3] {
    [BASE_VAULT_SEED, pool, bump]
}

/// Get seeds for quote vault PDA signing
#[inline]
pub fn quote_vault_seeds<'a>(pool: &'a [u8], bump: &'a [u8]) -> [&'a [u8]; 3] {
    [QUOTE_VAULT_SEED, pool, bump]
}
