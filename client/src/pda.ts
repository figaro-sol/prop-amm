import { PublicKey } from "@solana/web3.js";

export const POOL_SEED = Buffer.from("pool");
export const BASE_VAULT_SEED = Buffer.from("base_vault");
export const QUOTE_VAULT_SEED = Buffer.from("quote_vault");

/**
 * Find Pool PDA
 * Seeds: ["pool", base_mint, quote_mint]
 */
export function findPoolPda(
  programId: PublicKey,
  baseMint: PublicKey,
  quoteMint: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [POOL_SEED, baseMint.toBuffer(), quoteMint.toBuffer()],
    programId
  );
}

/**
 * Find Base Vault PDA
 * Seeds: ["base_vault", pool]
 */
export function findBaseVaultPda(
  programId: PublicKey,
  pool: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [BASE_VAULT_SEED, pool.toBuffer()],
    programId
  );
}

/**
 * Find Quote Vault PDA
 * Seeds: ["quote_vault", pool]
 */
export function findQuoteVaultPda(
  programId: PublicKey,
  pool: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [QUOTE_VAULT_SEED, pool.toBuffer()],
    programId
  );
}
