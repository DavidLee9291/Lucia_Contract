use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{ self, Mint, Token, TokenAccount, Transfer };

mod calculate;

declare_id!("3V3KvTQz94y5TGdYtmwVDSe1aGYjh8m5GGxCvYYQyTXZ");

#[program]
pub mod lucia_vesting {
    use super::*;

    use calculate::*;

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
        data_account.initialized_at = Clock::get()?.unix_timestamp as u64;
        // LCD - 05
        data_account.is_initialized = 0; // 계정을 초기화된 상태로 표시
        msg!("before state : {}", data_account.is_initialized);

        // 계정이 이미 초기화되었는지 확인
        if data_account.is_initialized == 1 {
            return Err(VestingError::AlreadyInitialized.into());
        }

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
        // 초기화 되면 카운팅
        data_account.is_initialized += 1;
        msg!("After state : {}", data_account.is_initialized);
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
        let token_program = &ctx.accounts.token_program;
        let token_mint_key = &ctx.accounts.token_mint.key();
        let beneficiary_ata = &ctx.accounts.wallet_to_deposit_to;
        let decimals = data_account.decimals;
        let state = data_account.state;
        let initialize_at = data_account.initialized_at;

        msg!("Claim Lux!! {:?}", beneficiary_ata);

        msg!("initialize_at : {}", initialize_at);

        if state == 0 {
            return Err(VestingError::ReleaseNotCalled.into());
        }

        let (index, beneficiary) = beneficiaries
            .iter()
            .enumerate()
            .find(|(_, beneficiary)| beneficiary.key == *sender.to_account_info().key)
            .ok_or(VestingError::BeneficiaryNotFound)?;

        let allocated_tokens = beneficiary.allocated_tokens;

        let current_time = Clock::get()?.unix_timestamp as i64;
        let lockup_end_time = (data_account.initialized_at as i64) + beneficiary.lockup_period;

        if current_time < lockup_end_time {
            msg!("Lockup period has not expired");
            return Err(VestingError::LockupNotExpired.into());
        }

        let vesting_end_month = beneficiary.vesting_end_month;
        let confirm_round = beneficiary.confirm_round;

        let schedule = calculate_schedule(
            lockup_end_time as i64,
            vesting_end_month as i64,
            beneficiary.unlock_duration as i64,
            allocated_tokens as i64,
            confirm_round
        );

        let mut total_claimable_tokens: u64 = 0;

        for item in schedule {
            let item1 = &item.0;
            let item2 = item.1;
            let item3 = item.2;

            let round_num = item1.split(": ").nth(1).unwrap().parse::<u64>().unwrap();
            if current_time >= item2 && (confirm_round as u8) <= (round_num as u8) {
                msg!(
                    "토큰 청구 가능:  {}, timestamp: {}, claimable_token: {}",
                    item1,
                    item2,
                    item3
                );
                total_claimable_tokens += item3 as u64;
            } else {
                msg!(
                    "토큰 청구 불가능:  {}, timestamp: {}, claimable_token: {}",
                    item1,
                    item2,
                    item3
                );
            }
        }

        if total_claimable_tokens > 0 {
            msg!("총 청구 가능한 토큰: {}", total_claimable_tokens);
        }

        let amount_to_transfer = match
            total_claimable_tokens.checked_mul(u64::pow(10, decimals as u32))
        {
            Some(value) => value,
            None => {
                msg!("Overflow occurred during amount calculation");
                return Err(VestingError::InvalidArgument.into());
            }
        };

        msg!("전송할 금액: {}", amount_to_transfer);

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

    // LCD - 10
    #[account(
        init,
        payer = sender,
        seeds = [b"escrow_wallet", token_mint.key().as_ref()],
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
    pub lockup_period: i64, // 8
    pub unlock_duration: u64, // 8
    pub vesting_end_month: u64, // 8
    pub confirm_round: u8, // 1
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
    pub is_initialized: u8, // 1
    pub contract_end_month: u8, // 1
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
    #[msg("Invalid argument encountered")]
    InvalidArgument,
    #[msg("Release function has not been called after initialization.")]
    ReleaseNotCalled,
    #[msg("Invalid token mint.")]
    InvalidTokenMint,
    #[msg("The program has already been initialized.")]
    AlreadyInitialized,
}
