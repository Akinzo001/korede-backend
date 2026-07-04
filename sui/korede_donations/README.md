# Korede Donations Sui Package

This Sui Move package records public donation proofs for Korede.

It does not move donor money and does not store private medical data. The backend should send hashed identifiers for `case_id`, `hospital_id`, and `payment_reference`.

## Local Commands

Run these from this directory after installing Sui CLI:

```bash
sui move build
sui move test
```

## Publish To Testnet

Configure Sui testnet and fund your address:

```bash
sui client new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
sui client switch --env testnet
sui client active-address
sui client faucet
```

Publish:

```bash
sui client publish --gas-budget 10000000
```

After publishing, copy the package ID into your backend environment:

```env
SUI_PACKAGE_ID=0x...
SUI_ADMIN_ADDRESS=0x...
SUI_KEYSTORE_PATH=/path/to/sui.keystore
SUI_CLOCK_OBJECT_ID=0x6
SUI_REQUEST_TIMEOUT_SECONDS=30
```

## Backend Proof Publishing

The backend now submits donation proof transactions through the local `sui` CLI.

For local backend development, leave `SUI_PACKAGE_ID`, `SUI_ADMIN_ADDRESS`, and
`SUI_KEYSTORE_PATH` blank. The backend will still start, confirmed donations will
remain recorded in PostgreSQL, and proof publishing will be unavailable until
Sui configuration is provided.

For an environment that should publish proofs, install the Sui CLI on the host
running the backend, configure the admin address in the keystore, and set the
Sui environment variables above. Failed proof submissions are retried by the
backend worker using the retry state stored on each donation.
