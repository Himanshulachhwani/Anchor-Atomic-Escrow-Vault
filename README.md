# Anchor-Atomic-Escrow-Vault

A stateless, trustless atomic swap engine on Solana using Program Derived Addresses (PDAs) and Cross-Program Invocations for secure SPL token exchanges.

## Overview

Anchor Atomic Escrow Vault is a fully non-custodial, peer-to-peer decentralized exchange protocol built natively on Solana using the Anchor framework. It enables atomic swaps between two arbitrary SPL tokens without trusting an intermediary or liquidity pool.

## Architecture & Solana Mechanics

- **Stateless Execution:** State is cleanly decoupled from program logic. Each trade allocates a dedicated 113-byte Escrow PDA derived deterministically from `[b"escrow", maker_pubkey, seed]`.
- **Zero Locked Rent:** Escrow PDA accounts and vault token accounts are rent-exempt. Upon swap completion (`take`) or cancellation (`refund`), the program closes both accounts and refunds 100% of the rent lamports back to the Maker.
- **Cross-Program Invocations (CPIs):** Transfers are executed via signed CPIs to the SPL Token Program, ensuring tokens never leave program-derived escrow authority without meeting exact on-chain terms.

## Instruction Set

1. `make(seed, deposit_amount, receive_amount)`: Initializes the escrow PDA, sets exchange rates, and deposits Maker's tokens into the vault.
2. `take()`: Taker fulfills the requested tokens to Maker; program atomically releases vault tokens to Taker and closes the escrow.
3. `refund()`: Allows Maker to cancel an open order at any time, returning all deposited tokens and closing the vault.
