use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("HWaGc49AZC3Aim9M1VBFAhgLdmw9Xp7T7QF4K9kgpuLh");

#[program]
pub mod token_vesting {

    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        beneficiaries: Vec<Beneficiary>,
        amount: u64,
        decimals: u8,
        lockup_period: u64,
    ) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        msg!(
            "Initializing data account with amount: {}, decimals: {}, lockup_duration: {}",
            amount,
            decimals,
            lockup_period
        );

        data_account.beneficiaries = beneficiaries;
        data_account.percent_available = 0;
        data_account.token_amount = amount;
        data_account.decimals = decimals; // b/c bpf does not have any floats
        data_account.initializer = ctx.accounts.sender.to_account_info().key();
        data_account.escrow_wallet = ctx.accounts.escrow_wallet.to_account_info().key();
        data_account.token_mint = ctx.accounts.token_mint.to_account_info().key();
        data_account.lockup_end_time = Clock::get()?.unix_timestamp + lockup_period as i64; // lockup 종료 시간 설정

        msg!(
            "Data account initialized with lockup end time: {}",
            data_account.lockup_end_time
        );

        let transfer_instruction: Transfer = Transfer {
            from: ctx.accounts.wallet_to_withdraw_from.to_account_info(),
            to: ctx.accounts.escrow_wallet.to_account_info(),
            authority: ctx.accounts.sender.to_account_info(),
        };

        let cpi_ctx: CpiContext<Transfer> = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_instruction,
        );

        token::transfer(
            cpi_ctx,
            data_account.token_amount * u64::pow(10, decimals as u32),
        )?;

        msg!("Token transfer completed");

        Ok(())
    }

    //
    pub fn release_lucia_vesting(
        ctx: Context<Release>,
        _data_bump: u8,
        percent: u8,
        base_claim_percentage: f32,
        initial_bonus_percentage: f32,
    ) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        data_account.percent_available = percent;
        data_account.base_claim_percentage = base_claim_percentage;
        data_account.initial_bonus_percentage = initial_bonus_percentage;
        Ok(())
    }

    pub fn claim_lucia_token(ctx: Context<Claim>, data_bump: u8, _escrow_bump: u8) -> Result<()> {
        let sender: &Signer = &ctx.accounts.sender;
        let escrow_wallet: &Account<TokenAccount> = &ctx.accounts.escrow_wallet;
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;
        let beneficiaries: &Vec<Beneficiary> = &data_account.beneficiaries;
        let token_program: &Program<Token> = &ctx.accounts.token_program;
        let token_mint_key: &Pubkey = &ctx.accounts.token_mint.key();
        let beneficiary_ata: &Account<TokenAccount> = &ctx.accounts.wallet_to_deposit_to;
        let decimals = data_account.decimals;

        // Get the current time
        let current_time = Clock::get()?.unix_timestamp;

        // Check if the lockup period has ended
        require!(
            current_time >= data_account.lockup_end_time,
            VestingError::LockupNotExpired
        );

        let (index, beneficiary) = beneficiaries
            .iter()
            .enumerate()
            .find(|(_, beneficiary)| beneficiary.key == *sender.to_account_info().key)
            .ok_or(VestingError::BeneficiaryNotFound)?;

        let lockup_end_time = data_account.lockup_end_time;
        let time_since_lockup_end = current_time - lockup_end_time;

        // Basic claim percentage and initial bonus percentage
        let base_claim_percentage = data_account.base_claim_percentage;
        let initial_bonus_percentage = data_account.initial_bonus_percentage;

        // Calculate the claimable percentage
        let claimable_percentage = if time_since_lockup_end < 30 * 24 * 60 * 60 {
            initial_bonus_percentage
        } else {
            let months_since_lockup_end = (time_since_lockup_end / (30 * 24 * 60 * 60)) as f32;
            base_claim_percentage * (months_since_lockup_end + 1.0).min(12.0)
        };

        let total_claimable_tokens =
            ((claimable_percentage / 100.0) * (beneficiary.allocated_tokens as f32)) as u64;

        msg!("total_claimable: {}", total_claimable_tokens);
        msg!("beneficiary: {}", beneficiary.claimed_tokens);

        let amount_to_transfer = total_claimable_tokens.saturating_sub(beneficiary.claimed_tokens);

        // Prevent double claim: check if the claimable tokens are greater than 0
        require!(amount_to_transfer > 0, VestingError::ClaimNotAllowed);

        // Transfer Logic:
        let seeds = &[
            "lucia_data_account".as_bytes(),
            token_mint_key.as_ref(),
            &[data_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let transfer_instruction = Transfer {
            from: escrow_wallet.to_account_info(),
            to: beneficiary_ata.to_account_info(),
            authority: data_account.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            token_program.to_account_info(),
            transfer_instruction,
            signer_seeds,
        );

        token::transfer(cpi_ctx, amount_to_transfer * u64::pow(10, decimals as u32))?;
        data_account.beneficiaries[index].claimed_tokens += amount_to_transfer;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = sender,
        space = 8 + 1 + 8 + 32 + 32 + 32 + 1 + 8 + 8 + (4 + 50 * (32 + 8 + 8) + 1), // Can take 50 accounts to vest to
        seeds = [b"lucia_data_account", token_mint.key().as_ref()],
        bump
    )]
    pub data_account: Account<'info, DataAccount>,

    #[account(
        init,
        payer = sender,
        seeds = [b"lucia_escrow_wallet".as_ref(), token_mint.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = data_account
    )]
    pub escrow_wallet: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint=wallet_to_withdraw_from.owner == sender.key(),
        constraint=wallet_to_withdraw_from.mint == token_mint.key()
    )]
    pub wallet_to_withdraw_from: Account<'info, TokenAccount>,

    pub token_mint: Account<'info, Mint>,

    #[account(mut)]
    pub sender: Signer<'info>,

    pub system_program: Program<'info, System>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(data_bump: u8)]
pub struct Release<'info> {
    #[account(
        mut,
        seeds = [b"lucia_data_account", token_mint.key().as_ref()], 
        bump = data_bump,
        constraint=data_account.initializer == sender.key() @ VestingError::InvalidSender
    )]
    pub data_account: Account<'info, DataAccount>,

    pub token_mint: Account<'info, Mint>,

    #[account(mut)]
    pub sender: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(data_bump: u8, wallet_bump: u8)]
pub struct Claim<'info> {
    #[account(
        mut,
        seeds = [b"lucia_data_account", token_mint.key().as_ref()], 
        bump = data_bump,
    )]
    pub data_account: Account<'info, DataAccount>,

    #[account(
        mut,
        seeds=[b"lucia_escrow_wallet".as_ref(), token_mint.key().as_ref()],
        bump = wallet_bump,
    )]
    pub escrow_wallet: Account<'info, TokenAccount>,

    #[account(mut)]
    pub sender: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = token_mint,
        associated_token::authority = sender
    )]
    pub wallet_to_deposit_to: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>, // Don't actually use it in the instruction, but used for the wallet_to_deposit_to account

    pub token_program: Program<'info, Token>,

    pub system_program: Program<'info, System>,
}

#[derive(Default, Copy, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct Beneficiary {
    pub key: Pubkey,           // 32
    pub allocated_tokens: u64, // 8
    pub claimed_tokens: u64,   // 8
}

#[account]
#[derive(Default)]
pub struct DataAccount {
    // Space in bytes: 8 + 1 + 8 + 32 + 32 + 32 + 1 + 8 + 8 + 8 + (4 + (100 * (32 + 8 + 8)))
    pub percent_available: u8,           // 1
    pub token_amount: u64,               // 8
    pub initializer: Pubkey,             // 32
    pub escrow_wallet: Pubkey,           // 32
    pub token_mint: Pubkey,              // 32
    pub beneficiaries: Vec<Beneficiary>, // (4 + (n * (32 + 8 + 8)))
    pub decimals: u8,                    // 1
    pub lockup_end_time: i64,            // 8
    pub base_claim_percentage: f32,      // 8
    pub initial_bonus_percentage: f32,   // 8
}

#[error_code]
pub enum VestingError {
    #[msg("Sender is not owner of Data Account")]
    InvalidSender,
    #[msg("Not allowed to claim new tokens currently")]
    ClaimNotAllowed,
    #[msg("Beneficiary does not exist in account")]
    BeneficiaryNotFound,
    #[msg("Lockup period has not expired yet.")]
    LockupNotExpired,
}
