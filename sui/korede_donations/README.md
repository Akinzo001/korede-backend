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
sui client new-env --alias testnet --rpc https://sui-testnet.grpc.ankr.com:443
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
SUI_CLI_PATH=sui
SUI_RPC_URL=https://sui-testnet.grpc.ankr.com:443
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
Sui environment variables above. `SUI_CLI_PATH` defaults to `sui`, which means
the binary must be available on `PATH`; set it to an absolute executable path if
your deployment installs the CLI elsewhere. Failed proof submissions are retried
by the backend worker using the retry state stored on each donation.

## Render Docker Deployment

Production should run this backend with the repository Dockerfile so the same
Sui testnet CLI version is available on Render. Upload the Sui keystore as a
Render Secret File named `sui.keystore`; Render exposes it at
`/etc/secrets/sui.keystore`.

Use these Sui values on Render:

```env
SUI_CLI_PATH=/usr/local/bin/sui
SUI_NETWORK=testnet
SUI_RPC_URL=https://sui-testnet.grpc.ankr.com:443
SUI_PACKAGE_ID=0x...
SUI_ADMIN_ADDRESS=0x...
SUI_KEYSTORE_PATH=/etc/secrets/sui.keystore
SUI_CLIENT_CONFIG_PATH=/tmp/korede-sui/client.yaml
SUI_GAS_BUDGET=10000000
SUI_CLOCK_OBJECT_ID=0x6
SUI_REQUEST_TIMEOUT_SECONDS=30
```

The Docker entrypoint copies the secret keystore into writable runtime storage
and generates the Sui client YAML at startup. The generated files are disposable
and are not committed or baked into the image.
