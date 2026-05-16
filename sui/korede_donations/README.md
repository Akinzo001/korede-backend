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
```

The backend does not need Sui CLI until a later task adds transaction submission.
