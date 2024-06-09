use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{ self, Mint, Token, TokenAccount, Transfer };

mod calculate;

declare_id!("bnQjwmPwzxn4mo2o26WyPn6oNfVVW4S6EJQvL3U3egz");

#[program]
pub mod lucia_vesting {
    use super::*;

    use calculate::*;

    // Initialize function to set up the vesting contract
    pub fn initialize(
        ctx: Context<Initialize>,
        beneficiaries: Vec<Beneficiary>,
        amount: u64,
        decimals: u8
    ) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        msg!("Initializing data account with amount: {}, decimals: {}", amount, decimals);
        msg!("Beneficiaries: {:?}", beneficiaries);

        // Validate inputs
        if ctx.accounts.token_mint.decimals != decimals {
            return Err(VestingError::InvalidDecimals.into());
        }

        if amount > ctx.accounts.wallet_to_withdraw_from.amount {
            return Err(VestingError::InsufficientFunds.into());
        }

        // LCD - 07
        if beneficiaries.len() > 50 {
            return Err(VestingError::TooManyBeneficiaries.into());
        }

        data_account.beneficiaries = beneficiaries;
        data_account.state = 0;
        data_account.token_amount = amount;
        data_account.decimals = decimals; // Because BPF does not support floats
        data_account.initializer = ctx.accounts.sender.key();
        data_account.escrow_wallet = ctx.accounts.escrow_wallet.key();
        data_account.token_mint = ctx.accounts.token_mint.key();
        // LCD - 01
        data_account.initialized_at = Clock::get()?.unix_timestamp as u64;
        data_account.is_initialized = 0; // Mark account as uninitialized

        msg!("Before state: {}", data_account.is_initialized);

        // LCD - 05
        // Check if the account has already been initialized
        if data_account.is_initialized == 1 {
            return Err(VestingError::AlreadyInitialized.into());
        }

        let transfer_instruction = Transfer {
            from: ctx.accounts.wallet_to_withdraw_from.to_account_info(),
            to: ctx.accounts.escrow_wallet.to_account_info(),
            authority: ctx.accounts.sender.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_instruction
        );

        token::transfer(cpi_ctx, data_account.token_amount * u64::pow(10, decimals as u32))?;
        data_account.is_initialized += 1; // Mark account as initialized

        msg!("After state: {}", data_account.is_initialized);
        msg!("Token transfer completed");

        Ok(())
    }

    // Release function to update the state of the vesting contract
    pub fn release_lucia_vesting(ctx: Context<Release>, _data_bump: u8, state: u8) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        data_account.state = state;

        msg!("Vesting Start - state: {}", state);

        Ok(())
    }

    // Claim function to allow beneficiaries to claim their vested tokens
    pub fn claim_lux(ctx: Context<Claim>, data_bump: u8, _escrow_bump: u8) -> Result<()> {
        let sender = &mut ctx.accounts.sender;
        let escrow_wallet = &mut ctx.accounts.escrow_wallet;
        let data_account = &mut ctx.accounts.data_account;
        let beneficiaries = &data_account.beneficiaries;
        let token_program = &ctx.accounts.token_program;
        let token_mint_key = &ctx.accounts.token_mint.key();
        let beneficiary_ata = &ctx.accounts.wallet_to_deposit_to;
        let decimals = data_account.decimals;
        let state = data_account.state;
        let initialized_at = data_account.initialized_at;

        msg!("Claim Lux!! {:?}", beneficiary_ata);
        msg!("Initialized at: {}", initialized_at);

        // LCD - 03
        if state == 0 {
            return Err(VestingError::ReleaseNotCalled.into());
        }

        let (index, beneficiary) = beneficiaries
            .iter()
            .enumerate()
            .find(|(_, beneficiary)| beneficiary.key == *sender.key)
            .ok_or(VestingError::BeneficiaryNotFound)?;

        let allocated_tokens = beneficiary.allocated_tokens;
        let current_time = Clock::get()?.unix_timestamp;
        let lockup_end_time = (initialized_at as i64) + beneficiary.lockup_period;

        if current_time < lockup_end_time {
            msg!("Lockup period has not expired");
            return Err(VestingError::LockupNotExpired.into());
        }

        let vesting_end_month = beneficiary.vesting_end_month;
        let confirm_round = beneficiary.confirm_round;

        // LCD - 02
        let schedule = calculate_schedule(
            lockup_end_time,
            vesting_end_month as i64,
            beneficiary.unlock_duration as i64,
            allocated_tokens as i64,
            confirm_round
        );

        let mut total_claimable_tokens: u64 = 0;

        for item in schedule {
            let round_num = item.0.split(": ").nth(1).unwrap().parse::<u64>().unwrap();
            if current_time >= item.1 && (confirm_round as u64) <= round_num {
                msg!(
                    "Tokens claimable: {}, timestamp: {}, claimable_token: {}",
                    item.0,
                    item.1,
                    item.2
                );
                total_claimable_tokens += item.2 as u64;
            } else {
                msg!(
                    "Tokens not claimable: {}, timestamp: {}, claimable_token: {}",
                    item.0,
                    item.1,
                    item.2
                );
            }
            // LCD - 06
            if vesting_end_month == round_num && current_time > item.1 {
                msg!("Vesting has ended, no more tokens can be claimed.");
            }
        }

        if total_claimable_tokens > 0 {
            msg!("Total claimable tokens: {}", total_claimable_tokens);
        }

        let amount_to_transfer = total_claimable_tokens
            .checked_mul(u64::pow(10, decimals as u32))
            .ok_or(VestingError::Overflow)?;

        msg!("Amount to transfer: {}", amount_to_transfer);

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

        token::transfer(cpi_ctx, amount_to_transfer)?;

        data_account.beneficiaries[index].claimed_tokens += amount_to_transfer;

        msg!("TEST: {}", amount_to_transfer);

        Ok(())
    }
}

// Context for Initialize function
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = sender,
        space = 8 + 1 + 8 + 32 + 32 + 32 + 8 + 1 + (4 + 50 * (32 + 8 + 8 + 8 + 4 + 8) + 1),
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump
    )]
    pub data_account: Account<'info, DataAccount>, // Data account to initialize

    // LCD - 10
    #[account(
        init,
        payer = sender,
        seeds = [b"escrow_wallet", token_mint.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = data_account
    )]
    pub escrow_wallet: Account<'info, TokenAccount>, // Escrow wallet account

    #[account(
        mut,
        constraint = wallet_to_withdraw_from.owner == sender.key(),
        constraint = wallet_to_withdraw_from.mint == token_mint.key()
    )]
    pub wallet_to_withdraw_from: Account<'info, TokenAccount>, // Account to withdraw tokens from

    pub token_mint: Account<'info, Mint>, // Token mint account

    #[account(mut)]
    pub sender: Signer<'info>, // Signer account

    pub system_program: Program<'info, System>, // System program account

    pub token_program: Program<'info, Token>, // Token program account
}

// Context for Release function
#[derive(Accounts)]
#[instruction(data_bump: u8)]
pub struct Release<'info> {
    #[account(
        mut,
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump = data_bump,
        constraint=data_account.initializer == sender.key() @ VestingError::InvalidSender
    )]
    pub data_account: Account<'info, DataAccount>, // Data account to update

    pub token_mint: Account<'info, Mint>, // Token mint account

    #[account(mut)]
    pub sender: Signer<'info>, // Signer account

    pub system_program: Program<'info, System>, // System program account
}

// Context for Claim function
#[derive(Accounts)]
#[instruction(data_bump: u8, wallet_bump: u8)]
pub struct Claim<'info> {
    #[account(
        mut,
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump = data_bump,
    )]
    pub data_account: Account<'info, DataAccount>, // Data account to update

    #[account(
        mut,
        seeds = [b"escrow_wallet", token_mint.key().as_ref()],
        bump = wallet_bump,
    )]
    pub escrow_wallet: Account<'info, TokenAccount>, // Escrow wallet account

    #[account(mut)]
    pub sender: Signer<'info>, // Signer account

    pub token_mint: Account<'info, Mint>, // Token mint account

    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = token_mint,
        associated_token::authority = sender
    )]
    pub wallet_to_deposit_to: Account<'info, TokenAccount>, // Beneficiary's wallet to deposit to

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

// Struct to represent each beneficiary
#[derive(Default, Copy, Clone, AnchorSerialize, AnchorDeserialize, Debug)]
pub struct Beneficiary {
    pub key: Pubkey, // Beneficiary's public key
    pub allocated_tokens: u64, // Tokens allocated to the beneficiary
    pub claimed_tokens: u64, // Tokens claimed by the beneficiary
    pub unlock_tge: f32, // Unlock percentage at TGE (Token Generation Event)
    pub lockup_period: i64, // Lockup period in seconds
    pub unlock_duration: u64, // Unlock duration in seconds
    pub vesting_end_month: u64, // Vesting end month
    pub confirm_round: u8, // Confirmation round
}

// Struct to represent the data account
#[account]
#[derive(Default)]
pub struct DataAccount {
    pub state: u8, // State of the vesting contract
    pub token_amount: u64, // Total token amount
    pub initializer: Pubkey, // Public key of the initializer
    pub escrow_wallet: Pubkey, // Public key of the escrow wallet
    pub token_mint: Pubkey, // Public key of the token mint
    pub initialized_at: u64, // Initialization timestamp
    pub beneficiaries: Vec<Beneficiary>, // List of beneficiaries
    pub decimals: u8, // Token decimals
    pub is_initialized: u8, // Flag to check if account is initialized
    pub contract_end_month: u8, // Contract end month
}

// Enum to represent errors
#[error_code]
pub enum VestingError {
    // Access Control Errors
    #[msg("Sender is not owner of Data Account")]
    InvalidSender,
    #[msg("Unauthorized: Only the contract issuer can initialize the contract.")]
    Unauthorized,

    // Validation Errors
    #[msg("Invalid argument encountered")]
    InvalidArgument,
    #[msg("Invalid token mint.")]
    InvalidTokenMint,
    #[msg("InvalidDecimals: The provided decimals do not match the token mint decimals.")]
    InvalidDecimals,
    #[msg("TooManyBeneficiaries: The number of beneficiaries exceeds the maximum allowed (50).")]
    TooManyBeneficiaries,

    // State Errors
    #[msg("Not allowed to claim new tokens currently")]
    ClaimNotAllowed,
    #[msg("Release function has not been called after initialization.")]
    ReleaseNotCalled,
    #[msg("The program has already been initialized.")]
    AlreadyInitialized,

    // Operational Errors
    #[msg("Beneficiary does not exist in account")]
    BeneficiaryNotFound,
    #[msg("Lockup period has not expired yet.")]
    LockupNotExpired,
    #[msg("InsufficientFunds: The sender's account does not have enough funds.")]
    InsufficientFunds,
    #[msg("Overflow: An overflow occurred during calculations.")]
    Overflow,
}
