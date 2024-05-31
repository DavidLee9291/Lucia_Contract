use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{ self, Mint, Token, TokenAccount, Transfer };

declare_id!("2VYbdzLVgza18y5jABNSZLkvrL1k9ksLCEqSmue1Xq3W");

#[program]
pub mod lucia_vesting {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        beneficiaries: Vec<Beneficiary>,
        amount: u64,
        decimals: u8
    ) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        msg!("Initializing data account with amount: {}, decimals: {}", amount, decimals);
        msg!("Beneficiaries: {:?}", beneficiaries);

        data_account.beneficiaries = beneficiaries;
        data_account.state = 0;
        data_account.token_amount = amount;
        data_account.decimals = decimals; // b/c bpf does not have any floats
        data_account.initializer = ctx.accounts.sender.to_account_info().key();
        data_account.escrow_wallet = ctx.accounts.escrow_wallet.to_account_info().key();
        data_account.token_mint = ctx.accounts.token_mint.to_account_info().key();

        let transfer_instruction: Transfer = Transfer {
            from: ctx.accounts.wallet_to_withdraw_from.to_account_info(),
            to: ctx.accounts.escrow_wallet.to_account_info(),
            authority: ctx.accounts.sender.to_account_info(),
        };

        let cpi_ctx: CpiContext<Transfer> = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_instruction
        );

        token::transfer(cpi_ctx, data_account.token_amount * u64::pow(10, decimals as u32))?;

        msg!("Token transfer completed");

        Ok(())
    }

    //
    pub fn release_lucia_vesting(ctx: Context<Release>, _data_bump: u8, state: u8) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        data_account.state = state;
        msg!("Vesting Start - state : {}", state);

        Ok(())
    }

    pub fn claim_lux(ctx: Context<Claim>, data_bump: u8, _escrow_bump: u8) -> Result<()> {
        let sender = &mut ctx.accounts.sender;
        let escrow_wallet = &mut ctx.accounts.escrow_wallet;
        let data_account = &mut ctx.accounts.data_account;
        let beneficiaries = &data_account.beneficiaries;
        let token_program = &mut ctx.accounts.token_program;
        let token_mint_key = &mut ctx.accounts.token_mint.key();
        let beneficiary_ata = &mut ctx.accounts.wallet_to_deposit_to;
        let decimals = data_account.decimals;

        msg!("Claim Lux!! {:?}", beneficiary_ata);
        // Find the beneficiary
        let (index, beneficiary) = beneficiaries
            .iter()
            .enumerate()
            .find(|(_, beneficiary)| beneficiary.key == *sender.to_account_info().key)
            .ok_or(VestingError::BeneficiaryNotFound)?;

        // Check if the lockup period has expired
        let current_time = Clock::get()?.unix_timestamp as u64;
        let lockup_end_time = data_account.initialized_at + beneficiary.lockup_period;
        require!(current_time >= lockup_end_time, VestingError::LockupNotExpired);

        // Calculate the unlockable tokens based on the unlock duration and unlockTge
        let time_since_lockup_end = current_time - lockup_end_time;
        let mut unlockable_tokens = 0;
        if time_since_lockup_end < beneficiary.unlock_duration {
            // 첫 번째 달인 경우 unlockTge 비율을 적용
            unlockable_tokens = (((beneficiary.unlock_tge as f64) / 100.0) *
                (beneficiary.allocated_tokens as f64)) as u64;
        } else {
            // 이후 달은 unlockDuration에 따라 선형적으로 토큰 지급
            let unlockable_duration = beneficiary.unlock_duration;
            let unlockable_percentage =
                (((time_since_lockup_end - unlockable_duration) as f64) /
                    (unlockable_duration as f64)) *
                100.0;
            unlockable_tokens = (((beneficiary.unlock_tge as f64) / 100.0 +
                unlockable_percentage / 100.0) *
                (beneficiary.allocated_tokens as f64)) as u64;
        }

        // Calculate the amount to transfer
        let amount_to_transfer = unlockable_tokens.saturating_sub(beneficiary.claimed_tokens);
        require!(amount_to_transfer > 0, VestingError::ClaimNotAllowed); // Allowed to claim new tokens

        // Transfer Logic:
        let seeds = &["data_account".as_bytes(), token_mint_key.as_ref(), &[data_bump]];
        let signer_seeds = &[&seeds[..]];

        let transfer_instruction = Transfer {
            from: escrow_wallet.to_account_info(),
            to: beneficiary_ata.to_account_info(),
            authority: data_account.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            token_program.to_account_info(),
            transfer_instruction,
            signer_seeds
        );

        token::transfer(cpi_ctx, amount_to_transfer * u64::pow(10, decimals as u32))?;

        // Update the claimed tokens for the beneficiary
        data_account.beneficiaries[index].claimed_tokens += amount_to_transfer;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = sender,
        space = 8 + 1 + 8 + 32 + 32 + 32 + 8 + 1 + (4 + 50 * (32 + 8 + 8 + 8 + 4 + 8) + 1), // Can take 50 accounts to vest to
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump
    )]
    pub data_account: Account<'info, DataAccount>,

    #[account(
        init,
        payer = sender,
        seeds = [b"escrow_wallet".as_ref(), token_mint.key().as_ref()],
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
        seeds = [b"data_account", token_mint.key().as_ref()], 
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
        seeds = [b"data_account", token_mint.key().as_ref()], 
        bump = data_bump,
    )]
    pub data_account: Account<'info, DataAccount>,

    #[account(
        mut,
        seeds = [b"escrow_wallet".as_ref(), token_mint.key().as_ref()],
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

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Default, Copy, Clone, AnchorSerialize, AnchorDeserialize, Debug)]
pub struct Beneficiary {
    pub key: Pubkey, // 32
    pub allocated_tokens: u64, // 8
    pub claimed_tokens: u64, // 8
    pub unlock_tge: f32, // 8
    pub lockup_period: u64, // 8
    pub unlock_duration: u64, // 8
}

#[account]
#[derive(Default)]
pub struct DataAccount {
    // Space in bytes: 8 + 1 + 8 + 32 + 32 + 32 + 8 + 1 + (4 + (100 * (32 + 8 + 8 + 8 + 8 + 8)))
    pub state: u8, // 1
    pub token_amount: u64, // 8
    pub initializer: Pubkey, // 32
    pub escrow_wallet: Pubkey, // 32
    pub token_mint: Pubkey, // 32
    pub initialized_at: u64, // 8
    pub beneficiaries: Vec<Beneficiary>, // (4 + (n * (32 + 8 + 8 + 8 + 8 + 8)))
    pub decimals: u8, // 1
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
