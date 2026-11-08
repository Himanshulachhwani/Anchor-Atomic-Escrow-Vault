use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

declare_id!("6Vo9FXTmNPiQrXah6SK68h71wEd4Xm1rQe5iTUtZQFs4");

#[program]
pub mod anchor_escrow {
    use super::*;

    /// 1. MAKE: Initializes the Escrow PDA and moves Maker's Token A into the Vault.
    pub fn make(
        ctx: Context<Make>,
        seed: u64,
        deposit_amount: u64,
        receive_amount: u64,
    ) -> Result<()> {
        require!(
            deposit_amount > 0 && receive_amount > 0,
            EscrowError::InvalidAmount
        );

        ctx.accounts.escrow.set_inner(Escrow {
            seed,
            maker: ctx.accounts.maker.key(),
            mint_a: ctx.accounts.mint_a.key(),
            mint_b: ctx.accounts.mint_b.key(),
            receive_amount,
            bump: ctx.bumps.escrow,
        });

        let transfer_accounts = TransferChecked {
            from: ctx.accounts.maker_ata_a.to_account_info(),
            mint: ctx.accounts.mint_a.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.maker.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_accounts,
        );

        transfer_checked(cpi_ctx, deposit_amount, ctx.accounts.mint_a.decimals)
    }

    /// 2. TAKE: Taker sends Token B to Maker, receives Token A from Vault, and closes accounts.
    pub fn take(ctx: Context<Take>) -> Result<()> {
        let transfer_to_maker = TransferChecked {
            from: ctx.accounts.taker_ata_b.to_account_info(),
            mint: ctx.accounts.mint_b.to_account_info(),
            to: ctx.accounts.maker_ata_b.to_account_info(),
            authority: ctx.accounts.taker.to_account_info(),
        };

        let cpi_to_maker = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_to_maker,
        );
        transfer_checked(
            cpi_to_maker,
            ctx.accounts.escrow.receive_amount,
            ctx.accounts.mint_b.decimals,
        )?;

        let seed_bytes = ctx.accounts.escrow.seed.to_le_bytes();
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"escrow",
            ctx.accounts.maker.to_account_info().key.as_ref(),
            seed_bytes.as_ref(),
            &[ctx.accounts.escrow.bump],
        ]];

        let transfer_to_taker = TransferChecked {
            from: ctx.accounts.vault.to_account_info(),
            mint: ctx.accounts.mint_a.to_account_info(),
            to: ctx.accounts.taker_ata_a.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };

        let cpi_to_taker = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_to_taker,
            signer_seeds,
        );
        transfer_checked(
            cpi_to_taker,
            ctx.accounts.vault.amount,
            ctx.accounts.mint_a.decimals,
        )?;

        let close_vault = CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.maker.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };

        let cpi_close = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            close_vault,
            signer_seeds,
        );
        close_account(cpi_close)
    }

    /// 3. REFUND: Maker cancels an open escrow, reclaiming Token A and closing the PDA/Vault.
    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        let seed_bytes = ctx.accounts.escrow.seed.to_le_bytes();
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"escrow",
            ctx.accounts.maker.to_account_info().key.as_ref(),
            seed_bytes.as_ref(),
            &[ctx.accounts.escrow.bump],
        ]];

        let transfer_back = TransferChecked {
            from: ctx.accounts.vault.to_account_info(),
            mint: ctx.accounts.mint_a.to_account_info(),
            to: ctx.accounts.maker_ata_a.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };

        let cpi_transfer = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_back,
            signer_seeds,
        );
        transfer_checked(
            cpi_transfer,
            ctx.accounts.vault.amount,
            ctx.accounts.mint_a.decimals,
        )?;

        let close_vault = CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.maker.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };

        let cpi_close = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            close_vault,
            signer_seeds,
        );
        close_account(cpi_close)
    }
}

#[account]
#[derive(InitSpace)]
pub struct Escrow {
    pub seed: u64,
    pub maker: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub receive_amount: u64,
    pub bump: u8,
}

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Make<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init,
        payer = maker,
        space = 8 + Escrow::INIT_SPACE,
        seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,
    #[account(mut)]
    pub maker: SystemAccount<'info>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_a,
        associated_token::authority = taker,
        associated_token::token_program = token_program
    )]
    pub taker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = taker,
        associated_token::token_program = token_program
    )]
    pub taker_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_b,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = mint_a,
        has_one = mint_b,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Refund<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = mint_a,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum EscrowError {
    #[msg("Deposit and receive amounts must be greater than zero.")]
    InvalidAmount,
}