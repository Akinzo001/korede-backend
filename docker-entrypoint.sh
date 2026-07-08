#!/usr/bin/env sh
set -eu

runtime_dir="${KOREDE_SUI_RUNTIME_DIR:-/tmp/korede-sui}"
client_config_path="${SUI_CLIENT_CONFIG_PATH:-$runtime_dir/client.yaml}"
sui_cli_path="${SUI_CLI_PATH:-sui}"

sui_requested=false
if [ -n "${SUI_PACKAGE_ID:-}" ] || [ -n "${SUI_ADMIN_ADDRESS:-}" ]; then
  sui_requested=true
fi

if [ "$sui_requested" = "true" ]; then
  if [ -z "${SUI_PACKAGE_ID:-}" ]; then
    echo "Sui proof publishing is enabled, but SUI_PACKAGE_ID is missing." >&2
    exit 1
  fi

  if [ -z "${SUI_ADMIN_ADDRESS:-}" ]; then
    echo "Sui proof publishing is enabled, but SUI_ADMIN_ADDRESS is missing." >&2
    exit 1
  fi

  source_keystore_path="${SUI_KEYSTORE_PATH:-/etc/secrets/sui.keystore}"
  if [ ! -r "$source_keystore_path" ]; then
    echo "Sui proof publishing is enabled, but the keystore file is not readable at $source_keystore_path." >&2
    echo "Upload a Render Secret File named sui.keystore, or disable Sui by clearing SUI_PACKAGE_ID and SUI_ADMIN_ADDRESS." >&2
    exit 1
  fi

  if [ ! -x "$sui_cli_path" ] && ! command -v "$sui_cli_path" >/dev/null 2>&1; then
    echo "Sui proof publishing is enabled, but SUI_CLI_PATH is not executable: $sui_cli_path" >&2
    exit 1
  fi

  umask 077
  mkdir -p "$runtime_dir"
  mkdir -p "$(dirname "$client_config_path")"

  runtime_keystore_path="$runtime_dir/sui.keystore"
  if [ "$source_keystore_path" != "$runtime_keystore_path" ]; then
    cp "$source_keystore_path" "$runtime_keystore_path"
  fi

  printf '{}\n' > "$runtime_dir/external.keystore"
  printf '{}\n' > "$runtime_dir/external.aliases"

  cat > "$client_config_path" <<EOF
---
keystore:
  File: "$runtime_keystore_path"
external_keys:
  External: "$runtime_dir/external.keystore"
envs: []
active_env: ~
active_address: ~
EOF

  export SUI_KEYSTORE_PATH="$runtime_keystore_path"
  export SUI_CLIENT_CONFIG_PATH="$client_config_path"
fi

exec "$@"
