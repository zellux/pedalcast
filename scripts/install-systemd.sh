#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

binary_path="${PEDALCAST_BINARY_PATH:-/usr/local/bin/pedalcast}"
config_dir="${PEDALCAST_CONFIG_DIR:-/etc/pedalcast}"
config_path="${PEDALCAST_CONFIG_PATH:-${config_dir}/config.toml}"
service_path="${PEDALCAST_SERVICE_PATH:-/etc/systemd/system/pedalcast.service}"
config_source="${PEDALCAST_CONFIG_SOURCE:-${repo_root}/examples/config.toml}"

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME}/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  . "${HOME}/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust first: https://rustup.rs/" >&2
  exit 1
fi

if ! command -v systemctl >/dev/null 2>&1; then
  echo "error: systemctl not found. This installer targets systemd Linux hosts." >&2
  exit 1
fi

for command in btmon hcitool; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "error: ${command} not found. Install BlueZ tools first." >&2
    exit 1
  fi
done

if [[ ! -f "${config_source}" ]]; then
  echo "error: config source not found: ${config_source}" >&2
  exit 1
fi

cd "${repo_root}"
cargo build --release

sudo install -m 0755 "${repo_root}/target/release/pedalcast" "${binary_path}"
sudo mkdir -p "${config_dir}"

if [[ -f "${config_path}" && -z "${PEDALCAST_OVERWRITE_CONFIG:-}" ]]; then
  echo "Keeping existing config: ${config_path}"
  echo "Set PEDALCAST_OVERWRITE_CONFIG=1 to replace it with ${config_source}."
else
  sudo install -m 0644 "${config_source}" "${config_path}"
fi

sudo install -m 0644 "${repo_root}/deploy/pedalcast.service" "${service_path}"
sudo systemctl daemon-reload
sudo systemctl enable pedalcast.service
sudo systemctl restart pedalcast.service

echo "Pedalcast installed and started."
systemctl --no-pager --full status pedalcast.service
