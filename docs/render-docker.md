# Render Docker Deployment

This backend should run as a Docker-based Render web service when Sui proof
publishing is enabled. The image pins the Sui testnet CLI to `1.72.1`, matching
the version verified locally.

## Render Runtime

Use the existing Render web service if possible so the public URL stays the
same. Change the service runtime to Docker and keep the Dockerfile default
command. Set the health check path to:

```text
/health
```

Render provides `PORT`; do not set `APP_PORT` in production unless you have a
specific reason to override Render's port.

## Secret File

Upload one Render Secret File named:

```text
sui.keystore
```

Use the contents of your local Sui keystore. On Windows this is usually:

```text
C:\Users\Admin\.sui\sui_config\sui.keystore
```

Render mounts the file at:

```text
/etc/secrets/sui.keystore
```

The container's non-root application user belongs to group `1000`, allowing it
to read Render runtime secret files.

Never commit the keystore, generated client YAML, aliases, recovery phrase, or
local `.env` file.

## Required Sui Environment

```env
APP_HOST=0.0.0.0
SUI_CLI_PATH=/usr/local/bin/sui
SUI_NETWORK=testnet
SUI_RPC_URL=https://sui-testnet.grpc.ankr.com:443
SUI_PACKAGE_ID=<existing-testnet-package-id>
SUI_ADMIN_ADDRESS=0x5ed7a88017cd7a39f0c02cb4b21422e90d876806d3140ecac20c32bd8909b378
SUI_KEYSTORE_PATH=/etc/secrets/sui.keystore
SUI_CLIENT_CONFIG_PATH=/tmp/korede-sui/client.yaml
SUI_GAS_BUDGET=10000000
SUI_CLOCK_OBJECT_ID=0x6
SUI_REQUEST_TIMEOUT_SECONDS=30
```

Keep the existing database, JWT, Paystack, email, storage, and application
environment values unchanged.

## How Startup Works

The container starts as a non-root user. When Sui publishing is enabled, the
entrypoint requires `/etc/secrets/sui.keystore`, copies it into writable private
runtime storage at `/tmp/korede-sui/sui.keystore`, generates
`/tmp/korede-sui/client.yaml`, and then starts the backend.

This keeps the wallet outside the image while still giving the Sui CLI writable
client state. No persistent disk is required because the generated Sui client
configuration is recreated on every startup.

If `SUI_PACKAGE_ID` and `SUI_ADMIN_ADDRESS` are blank, Sui publishing is treated
as disabled and the backend can still start without the secret file. Payments
will remain recorded, and proof publication will stay unavailable until Sui is
configured again.

## Local Docker Smoke Test

Start Docker Desktop, then build:

```bash
docker build -t korede-backend .
```

Run with local environment values and the keystore mounted read-only:

```bash
docker run --rm -p 4000:4000 --env-file .env \
  -e APP_HOST=0.0.0.0 \
  -e APP_PORT=4000 \
  -e SUI_CLI_PATH=/usr/local/bin/sui \
  -e SUI_KEYSTORE_PATH=/etc/secrets/sui.keystore \
  -e SUI_CLIENT_CONFIG_PATH=/tmp/korede-sui/client.yaml \
  -v "$HOME/.sui/sui_config/sui.keystore:/etc/secrets/sui.keystore:ro" \
  korede-backend
```

Verify:

```text
http://localhost:4000/health
http://localhost:4000/health/db
http://localhost:4000/docs
```
