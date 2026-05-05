import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { expect } from "chai";
import { KoredeDonations } from "../target/types/korede_donations";

describe("korede_donations", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .KoredeDonations as Program<KoredeDonations>;

  const systemProgram = anchor.web3.SystemProgram.programId;

  function bytes32(seed: number): number[] {
    return Array.from({ length: 32 }, (_, index) => (seed + index) % 256);
  }

  function donationRecordPda(paymentReference: number[]): anchor.web3.PublicKey {
    const [pda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("donation"), Buffer.from(paymentReference)],
      program.programId
    );

    return pda;
  }

  it("records a donation", async () => {
    const caseId = bytes32(1);
    const hospitalId = bytes32(40);
    const paymentReference = bytes32(90);
    const amountKobo = new anchor.BN(25_000);
    const donor = anchor.web3.Keypair.generate().publicKey;
    const donationRecord = donationRecordPda(paymentReference);

    await program.methods
      .recordDonation(caseId, hospitalId, amountKobo, paymentReference, donor)
      .accounts({
        authority: provider.wallet.publicKey,
        donationRecord,
        systemProgram,
      })
      .rpc();

    const stored = await program.account.donationRecord.fetch(donationRecord);

    expect(stored.donor.toBase58()).to.equal(donor.toBase58());
    expect(stored.authority.toBase58()).to.equal(
      provider.wallet.publicKey.toBase58()
    );
    expect(stored.caseId).to.deep.equal(caseId);
    expect(stored.hospitalId).to.deep.equal(hospitalId);
    expect(stored.amountKobo.toString()).to.equal(amountKobo.toString());
    expect(stored.paymentReference).to.deep.equal(paymentReference);
    expect(stored.recordedAt.toNumber()).to.be.greaterThan(0);
    expect(stored.bump).to.be.a("number");
  });

  it("rejects zero donation amount", async () => {
    const paymentReference = bytes32(120);
    const donationRecord = donationRecordPda(paymentReference);

    try {
      await program.methods
        .recordDonation(
          bytes32(2),
          bytes32(50),
          new anchor.BN(0),
          paymentReference,
          anchor.web3.Keypair.generate().publicKey
        )
        .accounts({
          authority: provider.wallet.publicKey,
          donationRecord,
          systemProgram,
        })
        .rpc();

      expect.fail("Expected zero donation amount to be rejected");
    } catch (error) {
      expect(String(error)).to.include("InvalidDonationAmount");
    }
  });

  it("rejects an all-zero payment reference", async () => {
    const paymentReference = Array(32).fill(0);
    const donationRecord = donationRecordPda(paymentReference);

    try {
      await program.methods
        .recordDonation(
          bytes32(3),
          bytes32(60),
          new anchor.BN(5_000),
          paymentReference,
          anchor.web3.Keypair.generate().publicKey
        )
        .accounts({
          authority: provider.wallet.publicKey,
          donationRecord,
          systemProgram,
        })
        .rpc();

      expect.fail("Expected empty payment reference to be rejected");
    } catch (error) {
      expect(String(error)).to.include("InvalidPaymentReference");
    }
  });

  it("rejects duplicate payment references", async () => {
    const paymentReference = bytes32(150);
    const donationRecord = donationRecordPda(paymentReference);
    const donor = anchor.web3.Keypair.generate().publicKey;

    await program.methods
      .recordDonation(
        bytes32(4),
        bytes32(70),
        new anchor.BN(10_000),
        paymentReference,
        donor
      )
      .accounts({
        authority: provider.wallet.publicKey,
        donationRecord,
        systemProgram,
      })
      .rpc();

    try {
      await program.methods
        .recordDonation(
          bytes32(4),
          bytes32(70),
          new anchor.BN(10_000),
          paymentReference,
          donor
        )
        .accounts({
          authority: provider.wallet.publicKey,
          donationRecord,
          systemProgram,
        })
        .rpc();

      expect.fail("Expected duplicate payment reference to be rejected");
    } catch (error) {
      expect(String(error)).to.match(/already in use|custom program error/i);
    }
  });
});
