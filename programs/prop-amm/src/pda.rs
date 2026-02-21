use pinocchio::Address;

/// Seed prefix for Pool authority PDA (vault owner + c_u_soon delegation_authority)
pub const POOL_SEED: &[u8] = b"pool";

/// Create Pool authority PDA with known bump.
///
/// Seeds: ["pool", base_mint, quote_mint, bump]
///
/// Platform dispatch:
/// - On `target_os = "solana"` / `target_arch = "bpf"`: calls `Address::create_program_address`.
/// - In non-BPF tests: delegates to `solana_sdk::pubkey::Pubkey::create_program_address`.
/// - Non-BPF, non-test: panics (`unimplemented!`).
#[cfg(any(target_os = "solana", target_arch = "bpf"))]
pub fn create_pool_address_with_bump(
    program_id: &Address,
    base_mint: &[u8; 32],
    quote_mint: &[u8; 32],
    bump: u8,
) -> Option<Address> {
    Address::create_program_address(
        &[POOL_SEED, base_mint.as_ref(), quote_mint.as_ref(), &[bump]],
        program_id,
    )
    .ok()
}

#[cfg(all(not(any(target_os = "solana", target_arch = "bpf")), test))]
pub fn create_pool_address_with_bump(
    program_id: &Address,
    base_mint: &[u8; 32],
    quote_mint: &[u8; 32],
    bump: u8,
) -> Option<Address> {
    use solana_sdk::pubkey::Pubkey as SolanaPubkey;
    let program_pubkey = SolanaPubkey::new_from_array(program_id.to_bytes());
    SolanaPubkey::create_program_address(
        &[POOL_SEED, base_mint.as_ref(), quote_mint.as_ref(), &[bump]],
        &program_pubkey,
    )
    .map(|pk| Address::from(pk.to_bytes()))
    .ok()
}

#[cfg(all(not(any(target_os = "solana", target_arch = "bpf")), not(test)))]
pub fn create_pool_address_with_bump(
    _program_id: &Address,
    _base_mint: &[u8; 32],
    _quote_mint: &[u8; 32],
    _bump: u8,
) -> Option<Address> {
    unimplemented!("create_pool_address_with_bump only available on BPF or in tests")
}
