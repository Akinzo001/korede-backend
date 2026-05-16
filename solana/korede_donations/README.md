# Deprecated: Korede Donations Anchor Program

This Solana/Anchor project is kept temporarily for reference only.

Korede is moving blockchain proof recording to Sui. New blockchain proof work should happen under `sui/korede_donations`.

This Anchor project is intentionally separate from the Axum backend at the repo root.

The program records immutable donation proofs on Solana. It does not move donor funds and does not store private patient or medical data.

## Local Commands

Run these from this directory in WSL/Linux:

```bash
anchor build
anchor test
```

## Devnet Deployment

```bash
solana config set --url devnet
solana-keygen new
solana airdrop 2
anchor build
solana address -k target/deploy/korede_donations-keypair.json
```

Copy the generated program ID into:

- `Anchor.toml`
- `programs/korede_donations/src/lib.rs`
- root `.env.example` as `SOLANA_PROGRAM_ID`

Then:

```bash
anchor build
anchor deploy --provider.cluster devnet
solana program show <PROGRAM_ID> --url devnet
```

Do not commit Solana keypair JSON files.
