//! CU measurement and credit model verification tests.
//!
//! Requires compiled binaries:
//! - `cargo build-sbf` in prop-amm (produces prop_amm.so)
//! - `cargo build-sbf` in c_u_soon workspace (produces c_u_soon_program.so)

use bytemuck::{bytes_of, from_bytes};
use c_u_later::{to_authority_wire_mask, to_program_wire_mask};
use c_u_soon::{Envelope, OracleState, TypeHash, AUX_DATA_SIZE, ORACLE_BYTES};
use mollusk_svm::program::create_program_account_loader_v3;
use mollusk_svm::result::Check;
use mollusk_svm::Mollusk;
use pinocchio::Address;
use prop_amm::{
    math::{buy_base_piecewise, sell_base_piecewise},
    state::{PropAmmAux, PropAmmQuote},
};
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
};

const PROP_AMM_ID: Address = Address::new_from_array([
    0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
    0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
]);

const C_U_SOON_ID: Address = Address::new_from_array([
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
]);

const SPL_TOKEN_ID: Address = Address::new_from_array([
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93, 0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91, 0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
]);

const PROP_AMM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/prop_amm");

const C_U_SOON_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../target/deploy/c_u_soon_program"
);

const PRICE_SCALE: u64 = 1_000_000_000;

const BID_PRICES: [u64; 7] = [
    194 * PRICE_SCALE,
    195 * PRICE_SCALE,
    196 * PRICE_SCALE,
    197 * PRICE_SCALE,
    198 * PRICE_SCALE,
    199 * PRICE_SCALE,
    200 * PRICE_SCALE,
];

const ASK_PRICES: [u64; 7] = [
    201 * PRICE_SCALE,
    202 * PRICE_SCALE,
    203 * PRICE_SCALE,
    204 * PRICE_SCALE,
    205 * PRICE_SCALE,
    206 * PRICE_SCALE,
    207 * PRICE_SCALE,
];

const BID_TOTAL_SIZE: u64 = 2_000_000_000; // 2000 USDC (6 decimals)
const ASK_TOTAL_SIZE: u64 = 10_000_000_000; // 10 SOL (9 decimals)

// -- Helper functions --

fn setup_mollusk() -> Mollusk {
    let mut mollusk = Mollusk::new(&PROP_AMM_ID, PROP_AMM_PATH);
    mollusk.add_program(&C_U_SOON_ID, C_U_SOON_PATH);
    mollusk_svm_programs_token::token::add_program(&mut mollusk);
    mollusk
}

fn create_funded_account(lamports: u64) -> Account {
    Account {
        lamports,
        data: vec![],
        owner: Address::default(),
        executable: false,
        rent_epoch: 0,
    }
}

fn create_prop_amm_envelope(
    authority: &Address,
    pool_pda: &Address,
    quote: &PropAmmQuote,
    aux: &PropAmmAux,
) -> Account {
    let mut envelope = Envelope {
        authority: *authority,
        oracle_state: OracleState {
            oracle_metadata: PropAmmQuote::METADATA,
            sequence: 1,
            data: [0u8; ORACLE_BYTES],
            _pad: [0u8; 1],
        },
        bump: 0,
        _padding: [0u8; 7],
        delegation_authority: *pool_pda,
        program_bitmask: to_program_wire_mask::<PropAmmAux>(),
        user_bitmask: to_authority_wire_mask::<PropAmmAux>(),
        authority_aux_sequence: 0,
        program_aux_sequence: 0,
        auxiliary_metadata: PropAmmAux::METADATA,
        auxiliary_data: [0u8; AUX_DATA_SIZE],
    };

    let quote_bytes = bytes_of(quote);
    envelope.oracle_state.data[..quote_bytes.len()].copy_from_slice(quote_bytes);

    let aux_bytes = bytes_of(aux);
    envelope.auxiliary_data[..aux_bytes.len()].copy_from_slice(aux_bytes);

    Account {
        lamports: 1_000_000_000,
        data: bytes_of(&envelope).to_vec(),
        owner: C_U_SOON_ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn create_mint_account(decimals: u8) -> Account {
    let mut data = vec![0u8; 82];
    // mint_authority option = Some (1u32 LE)
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    // mint_authority pubkey
    data[4..36].copy_from_slice(&[0xFFu8; 32]);
    // supply
    data[36..44].copy_from_slice(&1_000_000_000_000u64.to_le_bytes());
    // decimals
    data[44] = decimals;
    // is_initialized
    data[45] = 1;

    Account {
        lamports: 1_000_000_000,
        data,
        owner: SPL_TOKEN_ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn create_token_account(mint: &Address, owner: &Address, amount: u64) -> Account {
    let mut data = vec![0u8; 165];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // state = Initialized
    data[108] = 1;

    Account {
        lamports: 1_000_000_000,
        data,
        owner: SPL_TOKEN_ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn create_swap_data(direction: u8, amount_in: u64, min_out: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(18);
    data.push(0u8); // Swap discriminator
    data.push(direction);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&min_out.to_le_bytes());
    data
}

fn create_fast_path_update_data(sequence: u64, quote: &PropAmmQuote) -> Vec<u8> {
    let mut data = Vec::with_capacity(16 + core::mem::size_of::<PropAmmQuote>());
    data.extend_from_slice(&PropAmmQuote::METADATA.as_u64().to_le_bytes());
    data.extend_from_slice(&sequence.to_le_bytes());
    data.extend_from_slice(bytes_of(quote));
    data
}

fn default_quote() -> PropAmmQuote {
    PropAmmQuote {
        bid_prices: BID_PRICES,
        ask_prices: ASK_PRICES,
    }
}

fn default_aux(
    base_mint: &Address,
    quote_mint: &Address,
    base_vault: &Address,
    quote_vault: &Address,
    pool_bump: u8,
) -> PropAmmAux {
    let mut aux = PropAmmAux {
        base_mint: [0u8; 32],
        quote_mint: [0u8; 32],
        base_vault: [0u8; 32],
        quote_vault: [0u8; 32],
        bid_total_size: BID_TOTAL_SIZE,
        ask_total_size: ASK_TOTAL_SIZE,
        is_active: 1,
        _pad_active: [0u8; 7],
        bid_credit: 0,
        bid_accumulated: 0,
        ask_credit: 0,
        ask_accumulated: 0,
        accumulated_at_seq: 1, // matches oracle sequence
        pool_authority_bump: pool_bump,
        _pad_bump: [0u8; 7],
    };
    aux.base_mint.copy_from_slice(base_mint.as_ref());
    aux.quote_mint.copy_from_slice(quote_mint.as_ref());
    aux.base_vault.copy_from_slice(base_vault.as_ref());
    aux.quote_vault.copy_from_slice(quote_vault.as_ref());
    aux
}

struct SwapSetup {
    user: Address,
    envelope_key: Address,
    user_base_key: Address,
    user_quote_key: Address,
    base_vault_key: Address,
    quote_vault_key: Address,
    base_mint_key: Address,
    quote_mint_key: Address,
    pool_pda: Address,
    pool_bump: u8,
    padding_key: Address,
    authority: Address,
}

impl SwapSetup {
    fn new() -> Self {
        let authority = Address::new_unique();
        let base_mint_key = Address::new_unique();
        let quote_mint_key = Address::new_unique();

        let (pool_pda, pool_bump) = Address::find_program_address(
            &[b"pool", base_mint_key.as_ref(), quote_mint_key.as_ref()],
            &PROP_AMM_ID,
        );

        Self {
            user: Address::new_unique(),
            envelope_key: Address::new_unique(),
            user_base_key: Address::new_unique(),
            user_quote_key: Address::new_unique(),
            base_vault_key: Address::new_unique(),
            quote_vault_key: Address::new_unique(),
            base_mint_key,
            quote_mint_key,
            pool_pda,
            pool_bump,
            padding_key: Address::new_unique(),
            authority,
        }
    }

    fn build_accounts(&self, aux: &PropAmmAux) -> Vec<(Address, Account)> {
        let quote = default_quote();
        let envelope = create_prop_amm_envelope(&self.authority, &self.pool_pda, &quote, aux);

        vec![
            (self.user, create_funded_account(1_000_000_000)),
            (self.authority, create_funded_account(1_000_000_000)),
            (self.envelope_key, envelope),
            (
                self.user_base_key,
                create_token_account(&self.base_mint_key, &self.user, 100_000_000_000),
            ),
            (
                self.user_quote_key,
                create_token_account(&self.quote_mint_key, &self.user, 100_000_000_000),
            ),
            (
                self.base_vault_key,
                create_token_account(&self.base_mint_key, &self.pool_pda, 100_000_000_000),
            ),
            (
                self.quote_vault_key,
                create_token_account(&self.quote_mint_key, &self.pool_pda, 100_000_000_000),
            ),
            (self.base_mint_key, create_mint_account(9)), // SOL-like (9 decimals)
            (self.quote_mint_key, create_mint_account(6)), // USDC-like (6 decimals)
            (
                SPL_TOKEN_ID,
                create_program_account_loader_v3(&SPL_TOKEN_ID),
            ),
            (C_U_SOON_ID, create_program_account_loader_v3(&C_U_SOON_ID)),
            (self.pool_pda, create_funded_account(0)),
            (self.padding_key, create_funded_account(0)),
        ]
    }

    fn build_swap_instruction(&self, direction: u8, amount_in: u64, min_out: u64) -> Instruction {
        Instruction::new_with_bytes(
            PROP_AMM_ID,
            &create_swap_data(direction, amount_in, min_out),
            vec![
                AccountMeta::new_readonly(self.user, true), // 0: user (signer)
                AccountMeta::new(self.envelope_key, false), // 1: envelope (writable)
                AccountMeta::new(self.user_base_key, false), // 2: user_base_account
                AccountMeta::new(self.user_quote_key, false), // 3: user_quote_account
                AccountMeta::new(self.base_vault_key, false), // 4: base_vault
                AccountMeta::new(self.quote_vault_key, false), // 5: quote_vault
                AccountMeta::new_readonly(self.base_mint_key, false), // 6: base_mint
                AccountMeta::new_readonly(self.quote_mint_key, false), // 7: quote_mint
                AccountMeta::new_readonly(SPL_TOKEN_ID, false), // 8: base_token_program
                AccountMeta::new_readonly(SPL_TOKEN_ID, false), // 9: quote_token_program
                AccountMeta::new_readonly(C_U_SOON_ID, false), // 10: c_u_soon_program
                AccountMeta::new_readonly(self.pool_pda, false), // 11: pool_authority_pda
                AccountMeta::new_readonly(self.padding_key, false), // 12: padding
            ],
        )
    }

    fn build_oracle_fast_update_instruction(&self, sequence: u64, quote: &PropAmmQuote) -> Instruction {
        Instruction::new_with_bytes(
            C_U_SOON_ID,
            &create_fast_path_update_data(sequence, quote),
            vec![
                AccountMeta::new_readonly(self.authority, true),
                AccountMeta::new(self.envelope_key, false),
            ],
        )
    }

    fn default_aux(&self) -> PropAmmAux {
        default_aux(
            &self.base_mint_key,
            &self.quote_mint_key,
            &self.base_vault_key,
            &self.quote_vault_key,
            self.pool_bump,
        )
    }
}

fn read_aux_from_result(
    result: &mollusk_svm::result::InstructionResult,
    envelope_key: &Address,
) -> PropAmmAux {
    let (_, envelope_account) = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| k == envelope_key)
        .expect("envelope not found in resulting accounts");

    let envelope: &Envelope = from_bytes(&envelope_account.data[..Envelope::SIZE]);
    *envelope.aux::<PropAmmAux>().expect("aux decode failed")
}

fn read_quote_from_result(
    result: &mollusk_svm::result::InstructionResult,
    envelope_key: &Address,
) -> PropAmmQuote {
    let (_, envelope_account) = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| k == envelope_key)
        .expect("envelope not found in resulting accounts");

    let envelope: &Envelope = from_bytes(&envelope_account.data[..Envelope::SIZE]);
    *envelope.oracle::<PropAmmQuote>().expect("quote decode failed")
}

fn read_oracle_sequence_from_result(
    result: &mollusk_svm::result::InstructionResult,
    envelope_key: &Address,
) -> u64 {
    let (_, envelope_account) = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| k == envelope_key)
        .expect("envelope not found in resulting accounts");

    let envelope: &Envelope = from_bytes(&envelope_account.data[..Envelope::SIZE]);
    envelope.oracle_state.sequence
}

fn read_token_amount_from_result(
    result: &mollusk_svm::result::InstructionResult,
    token_account_key: &Address,
) -> u64 {
    let (_, token_account) = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| k == token_account_key)
        .expect("token account not found in resulting accounts");

    u64::from_le_bytes(
        token_account.data[64..72]
            .try_into()
            .expect("token account amount slice"),
    )
}

// -- Tests --

#[test]
fn test_swap_buy_cu() {
    let mollusk = setup_mollusk();
    let setup = SwapSetup::new();
    let aux = setup.default_aux();
    let accounts = setup.build_accounts(&aux);

    let quote_in: u64 = 100_000_000; // 100 USDC
    let instruction = setup.build_swap_instruction(0, quote_in, 0);

    let result =
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    println!("========================================");
    println!("BuyBaseWithQuote CU: {}", result.compute_units_consumed);
    println!("========================================");

    assert!(
        result.compute_units_consumed < 50_000,
        "BuyBaseWithQuote used {} CUs, expected < 50,000",
        result.compute_units_consumed
    );
}

#[test]
fn test_swap_sell_cu() {
    let mollusk = setup_mollusk();
    let setup = SwapSetup::new();
    let aux = setup.default_aux();
    let accounts = setup.build_accounts(&aux);

    let base_in: u64 = 1_000_000_000; // 1 SOL
    let instruction = setup.build_swap_instruction(1, base_in, 0);

    let result =
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    println!("========================================");
    println!("SellBaseForQuote CU: {}", result.compute_units_consumed);
    println!("========================================");

    assert!(
        result.compute_units_consumed < 50_000,
        "SellBaseForQuote used {} CUs, expected < 50,000",
        result.compute_units_consumed
    );
}

#[test]
fn test_swap_credit_model() {
    let mollusk = setup_mollusk();
    let setup = SwapSetup::new();
    let aux = setup.default_aux();
    let accounts = setup.build_accounts(&aux);

    // Step 1: Buy swap (consume ask, credit bid via heal-then-grow)
    let quote_in: u64 = 100_000_000; // 100 USDC
    let buy_ix = setup.build_swap_instruction(0, quote_in, 0);

    // Step 2: Sell swap (consume bid, credit ask via heal-then-grow)
    let base_in: u64 = 500_000_000; // 0.5 SOL
    let sell_ix = setup.build_swap_instruction(1, base_in, 0);

    let result = mollusk.process_and_validate_instruction_chain(
        &[
            (&buy_ix, &[Check::success()]),
            (&sell_ix, &[Check::success()]),
        ],
        &accounts,
    );

    // Read final aux state from envelope
    let final_aux = read_aux_from_result(&result, &setup.envelope_key);

    // Compute expected state by replaying the math locally

    // After buy: consumes ask side, credits bid side
    let ask_effective = ASK_TOTAL_SIZE; // no credit yet
    let (base_bought, quote_used, new_ask_consumed) =
        buy_base_piecewise(quote_in, &ASK_PRICES, ask_effective, 0).unwrap();
    assert!(base_bought > 0, "buy should produce base");

    // Heal-then-grow on bid side: bid_accumulated=0, so all quote_used goes to bid_credit
    let expected_bid_credit_after_buy = quote_used;
    let expected_bid_accumulated_after_buy: u64 = 0;
    let expected_ask_accumulated_after_buy = new_ask_consumed;
    let expected_ask_credit_after_buy: u64 = 0;

    // After sell: consumes bid side (now with credit), credits ask side
    let bid_effective = BID_TOTAL_SIZE + expected_bid_credit_after_buy;
    let (quote_received, base_used, new_bid_consumed) = sell_base_piecewise(
        base_in,
        &BID_PRICES,
        bid_effective,
        expected_bid_accumulated_after_buy,
    )
    .unwrap();
    assert!(quote_received > 0, "sell should produce quote");

    let expected_bid_accumulated_final = new_bid_consumed;

    // Heal-then-grow on ask side: ask_accumulated=new_ask_consumed from buy
    let expected_ask_credit_final;
    let expected_ask_accumulated_final;
    if expected_ask_accumulated_after_buy >= base_used {
        expected_ask_accumulated_final = expected_ask_accumulated_after_buy - base_used;
        expected_ask_credit_final = expected_ask_credit_after_buy;
    } else {
        let remainder = base_used - expected_ask_accumulated_after_buy;
        expected_ask_accumulated_final = 0;
        expected_ask_credit_final = expected_ask_credit_after_buy + remainder;
    }

    // Verify
    assert_eq!(
        final_aux.bid_credit, expected_bid_credit_after_buy,
        "bid_credit mismatch: got {}, expected {}",
        final_aux.bid_credit, expected_bid_credit_after_buy
    );
    assert_eq!(
        final_aux.bid_accumulated, expected_bid_accumulated_final,
        "bid_accumulated mismatch: got {}, expected {}",
        final_aux.bid_accumulated, expected_bid_accumulated_final
    );
    assert_eq!(
        final_aux.ask_credit, expected_ask_credit_final,
        "ask_credit mismatch: got {}, expected {}",
        final_aux.ask_credit, expected_ask_credit_final
    );
    assert_eq!(
        final_aux.ask_accumulated, expected_ask_accumulated_final,
        "ask_accumulated mismatch: got {}, expected {}",
        final_aux.ask_accumulated, expected_ask_accumulated_final
    );

    println!("========================================");
    println!("Credit model verification passed:");
    println!(
        "  After buy: ask_acc={}, bid_credit={}",
        expected_ask_accumulated_after_buy, expected_bid_credit_after_buy
    );
    println!(
        "  After sell: bid_acc={}, ask_credit={}, ask_acc={}",
        expected_bid_accumulated_final, expected_ask_credit_final, expected_ask_accumulated_final
    );
    println!("  base_bought={}, quote_used={}", base_bought, quote_used);
    println!(
        "  quote_received={}, base_used={}",
        quote_received, base_used
    );
    println!("========================================");
}

#[test]
fn test_oracle_fast_path_update_then_swap_uses_new_prices() {
    let mollusk = setup_mollusk();
    let setup = SwapSetup::new();
    let aux = setup.default_aux();
    let accounts = setup.build_accounts(&aux);

    let updated_quote = PropAmmQuote {
        bid_prices: BID_PRICES,
        ask_prices: [
            301 * PRICE_SCALE,
            302 * PRICE_SCALE,
            303 * PRICE_SCALE,
            304 * PRICE_SCALE,
            305 * PRICE_SCALE,
            306 * PRICE_SCALE,
            307 * PRICE_SCALE,
        ],
    };

    let update_ix = setup.build_oracle_fast_update_instruction(2, &updated_quote);
    let quote_in: u64 = 100_000_000; // 100 USDC
    let swap_ix = setup.build_swap_instruction(0, quote_in, 0);

    let result = mollusk.process_and_validate_instruction_chain(
        &[
            (&update_ix, &[Check::success()]),
            (&swap_ix, &[Check::success()]),
        ],
        &accounts,
    );

    let final_quote = read_quote_from_result(&result, &setup.envelope_key);
    let final_aux = read_aux_from_result(&result, &setup.envelope_key);
    let final_oracle_sequence = read_oracle_sequence_from_result(&result, &setup.envelope_key);
    let user_base_amount = read_token_amount_from_result(&result, &setup.user_base_key);

    let (expected_base_out_new, quote_used, expected_ask_acc) =
        buy_base_piecewise(quote_in, &updated_quote.ask_prices, ASK_TOTAL_SIZE, 0)
            .expect("new quote math should succeed");
    let (expected_base_out_old, _, _) = buy_base_piecewise(quote_in, &ASK_PRICES, ASK_TOTAL_SIZE, 0)
        .expect("old quote math should succeed");

    assert_eq!(final_quote, updated_quote, "oracle quote should be fast-path updated");
    assert_eq!(final_oracle_sequence, 2, "oracle sequence should advance via c_u_soon");
    assert_eq!(
        user_base_amount,
        100_000_000_000 + expected_base_out_new,
        "swap should use updated ask prices for base output",
    );
    assert!(
        expected_base_out_new < expected_base_out_old,
        "higher ask prices should reduce base output",
    );

    assert_eq!(
        final_aux.accumulated_at_seq, 2,
        "swap should track latest oracle sequence",
    );
    assert_eq!(final_aux.ask_accumulated, expected_ask_acc);
    assert_eq!(final_aux.bid_credit, quote_used);
}
