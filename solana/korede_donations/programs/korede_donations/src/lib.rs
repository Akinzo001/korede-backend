use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWxTWqkCcN3DrjGJnQeqxYBzsY7S");

#[program]
pub mod korede_donations {
    use super::*;

    pub fn record_donation(
        ctx: Context<RecordDonation>,
        case_id: [u8; 32],
        hospital_id: [u8; 32],
        amount_kobo: u64,
        payment_reference: [u8; 32],
        donor: Pubkey,
    ) -> Result<()> {
        require!(amount_kobo > 0, KoredeDonationError::InvalidDonationAmount);
        require!(
            payment_reference != [0u8; 32],
            KoredeDonationError::InvalidPaymentReference
        );

        let donation_record = &mut ctx.accounts.donation_record;
        let clock = Clock::get()?;

        donation_record.donor = donor;
        donation_record.authority = ctx.accounts.authority.key();
        donation_record.case_id = case_id;
        donation_record.hospital_id = hospital_id;
        donation_record.amount_kobo = amount_kobo;
        donation_record.payment_reference = payment_reference;
        donation_record.recorded_at = clock.unix_timestamp;
        donation_record.bump = ctx.bumps.donation_record;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(
    case_id: [u8; 32],
    hospital_id: [u8; 32],
    amount_kobo: u64,
    payment_reference: [u8; 32],
    donor: Pubkey
)]
pub struct RecordDonation<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = DonationRecord::LEN,
        seeds = [b"donation", payment_reference.as_ref()],
        bump
    )]
    pub donation_record: Account<'info, DonationRecord>,

    pub system_program: Program<'info, System>,
}

#[account]
pub struct DonationRecord {
    pub donor: Pubkey,
    pub authority: Pubkey,
    pub case_id: [u8; 32],
    pub hospital_id: [u8; 32],
    pub amount_kobo: u64,
    pub payment_reference: [u8; 32],
    pub recorded_at: i64,
    pub bump: u8,
}

impl DonationRecord {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 8 + 32 + 8 + 1;
}

#[error_code]
pub enum KoredeDonationError {
    #[msg("Donation amount must be greater than zero.")]
    InvalidDonationAmount,

    #[msg("Payment reference must not be empty.")]
    InvalidPaymentReference,
}
