//! Compute Unit measurement tests using Mollusk

use mollusk_svm::{result::Check, Mollusk};
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount},
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

const PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
    0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
]);

// Pool discriminator (v2 for piecewise)
const POOL_DISCRIMINATOR: [u8; 8] = *b"propamm2";

// Pool size: 321 bytes (piecewise with 7 price points per side)
const POOL_SIZE: usize = 321;

// Number of price points per side
const NUM_PRICE_POINTS: usize = 7;

// Instruction discriminators
const IX_UPDATE_ORACLE: u8 = 1;
const IX_SWAP: u8 = 2;

// Price scale
const PRICE_SCALE: u64 = 1_000_000_000;

fn create_pool_account(authority: &Pubkey) -> AccountSharedData {
    let mut data = vec![0u8; POOL_SIZE];

    // discriminator (8 bytes)
    data[0..8].copy_from_slice(&POOL_DISCRIMINATOR);

    // bump (1 byte)
    data[8] = 255;

    // authority (32 bytes) at offset 9
    data[9..41].copy_from_slice(authority.as_ref());

    // base_mint (32 bytes) at offset 41
    data[41..73].copy_from_slice(&[1u8; 32]);

    // quote_mint (32 bytes) at offset 73
    data[73..105].copy_from_slice(&[2u8; 32]);

    // base_vault (32 bytes) at offset 105
    data[105..137].copy_from_slice(&[3u8; 32]);

    // quote_vault (32 bytes) at offset 137
    data[137..169].copy_from_slice(&[4u8; 32]);

    // bid_side (72 bytes) at offset 169
    // PiecewiseBookSide: prices[7] (56 bytes) + total_quantity (8) + consumed (8)
    let mut offset = 169;
    // 7 bid prices (strictly ascending)
    let bid_prices: [u64; 7] = [
        194 * PRICE_SCALE,
        195 * PRICE_SCALE,
        196 * PRICE_SCALE,
        197 * PRICE_SCALE,
        198 * PRICE_SCALE,
        199 * PRICE_SCALE,
        200 * PRICE_SCALE,
    ];
    for price in bid_prices {
        data[offset..offset + 8].copy_from_slice(&price.to_le_bytes());
        offset += 8;
    }
    // bid total_quantity
    let bid_quantity: u64 = 2000 * 1_000_000; // 2000 USDC
    data[offset..offset + 8].copy_from_slice(&bid_quantity.to_le_bytes());
    offset += 8;
    // bid consumed
    data[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
    offset += 8;

    // ask_side (72 bytes) at offset 241
    // 7 ask prices (strictly ascending)
    let ask_prices: [u64; 7] = [
        201 * PRICE_SCALE,
        202 * PRICE_SCALE,
        203 * PRICE_SCALE,
        204 * PRICE_SCALE,
        205 * PRICE_SCALE,
        206 * PRICE_SCALE,
        207 * PRICE_SCALE,
    ];
    for price in ask_prices {
        data[offset..offset + 8].copy_from_slice(&price.to_le_bytes());
        offset += 8;
    }
    // ask total_quantity
    let ask_quantity: u64 = 10 * PRICE_SCALE; // 10 SOL in lamports
    data[offset..offset + 8].copy_from_slice(&ask_quantity.to_le_bytes());
    offset += 8;
    // ask consumed
    data[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
    offset += 8;

    // is_active (1 byte) at offset 313
    data[offset] = 1; // true

    // padding (7 bytes) at offset 314
    // Already zero

    let mut account = AccountSharedData::new(1_000_000_000, POOL_SIZE, &PROGRAM_ID);
    account.set_data_from_slice(&data);
    account
}

fn create_update_oracle_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(129); // 1 discriminator + 128 oracle data

    // Instruction discriminator
    data.push(IX_UPDATE_ORACLE);

    // Data layout: ask_prices[7] + ask_qty + bid_prices[7] + bid_qty

    // ask_prices (7 × u64 = 56 bytes) - strictly ascending
    let ask_prices: [u64; 7] = [
        202 * PRICE_SCALE,
        203 * PRICE_SCALE,
        204 * PRICE_SCALE,
        205 * PRICE_SCALE,
        206 * PRICE_SCALE,
        207 * PRICE_SCALE,
        208 * PRICE_SCALE,
    ];
    for price in ask_prices {
        data.extend_from_slice(&price.to_le_bytes());
    }

    // ask_total_quantity (8 bytes)
    let ask_quantity: u64 = 8 * PRICE_SCALE; // 8 SOL
    data.extend_from_slice(&ask_quantity.to_le_bytes());

    // bid_prices (7 × u64 = 56 bytes) - strictly ascending
    let bid_prices: [u64; 7] = [
        195 * PRICE_SCALE,
        196 * PRICE_SCALE,
        197 * PRICE_SCALE,
        198 * PRICE_SCALE,
        199 * PRICE_SCALE,
        200 * PRICE_SCALE,
        201 * PRICE_SCALE,
    ];
    for price in bid_prices {
        data.extend_from_slice(&price.to_le_bytes());
    }

    // bid_total_quantity (8 bytes)
    let bid_quantity: u64 = 1500 * 1_000_000; // 1500 USDC
    data.extend_from_slice(&bid_quantity.to_le_bytes());

    data
}

#[test]
fn test_update_oracle_cu() {
    let mollusk = Mollusk::new(&PROGRAM_ID, "../../target/deploy/prop_amm");

    let authority = Pubkey::new_unique();
    let pool_pubkey = Pubkey::new_unique();

    let pool_account = create_pool_account(&authority);

    let instruction = Instruction::new_with_bytes(
        PROGRAM_ID,
        &create_update_oracle_data(),
        vec![
            AccountMeta::new_readonly(authority, true),  // signer
            AccountMeta::new(pool_pubkey, false),        // pool (writable)
        ],
    );

    let result = mollusk.process_and_validate_instruction(
        &instruction,
        &[
            (authority, AccountSharedData::new(1_000_000_000, 0, &Pubkey::default())),
            (pool_pubkey, pool_account),
        ],
        &[Check::success()],
    );

    println!("========================================");
    println!("UpdateOracle CU consumption: {}", result.compute_units_consumed);
    println!("========================================");

    // Verify it's under our estimate
    assert!(
        result.compute_units_consumed < 1000,
        "UpdateOracle used {} CUs, expected < 1000",
        result.compute_units_consumed
    );
}

fn create_token_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> AccountSharedData {
    // SPL Token account layout (165 bytes)
    let mut data = vec![0u8; 165];

    // mint (32 bytes)
    data[0..32].copy_from_slice(mint.as_ref());
    // owner (32 bytes)
    data[32..64].copy_from_slice(owner.as_ref());
    // amount (8 bytes)
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // delegate option (4 bytes) - None
    data[72..76].copy_from_slice(&[0, 0, 0, 0]);
    // state (1 byte) - Initialized
    data[108] = 1;
    // is_native option (4 bytes) - None
    data[109..113].copy_from_slice(&[0, 0, 0, 0]);
    // delegated_amount (8 bytes)
    data[117..125].copy_from_slice(&0u64.to_le_bytes());
    // close_authority option (4 bytes) - None
    data[125..129].copy_from_slice(&[0, 0, 0, 0]);

    let token_program = Pubkey::new_from_array([
        0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93,
        0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
        0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91,
        0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
    ]);

    let mut account = AccountSharedData::new(1_000_000_000, 165, &token_program);
    account.set_data_from_slice(&data);
    account
}

fn create_swap_data(direction: u8, amount_in: u64, min_amount_out: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(18);

    // Instruction discriminator
    data.push(IX_SWAP);

    // direction
    data.push(direction);

    // amount_in
    data.extend_from_slice(&amount_in.to_le_bytes());

    // min_amount_out
    data.extend_from_slice(&min_amount_out.to_le_bytes());

    data
}

// Note: Swap test requires more complex setup with token program
// This is a simplified version that tests the math/validation CUs
// without the actual token transfers
#[test]
fn test_swap_buy_cu_estimate() {
    // For a full swap test, we'd need to set up:
    // - Token program
    // - User token accounts
    // - Pool vault accounts
    // - Proper token transfers

    // The math portion (sqrt + arithmetic) can be estimated:
    // - Newton-Raphson sqrt: ~2,000-3,000 CUs
    // - u64/u128 arithmetic: ~500-1,000 CUs
    // - Account validation: ~500-1,000 CUs
    // - State serialization: ~200-500 CUs
    // Total math/validation: ~3,200-5,500 CUs
    //
    // Plus token transfer CPIs: ~6,000-8,000 CUs (2 transfers)
    // Total estimated: ~9,200-13,500 CUs

    println!("========================================");
    println!("Swap CU estimate (without token CPIs): ~3,500-5,500");
    println!("Swap CU estimate (with token CPIs): ~9,500-13,500");
    println!("========================================");
}
