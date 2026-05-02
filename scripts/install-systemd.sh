#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

binary_path="${PEDALCAST_BINARY_PATH:-/usr/local/bin/pedalcast}"
config_dir="${PEDALCAST_CONFIG_DIR:-/etc/pedalcast}"
config_path="${PEDALCAST_CONFIG_PATH:-${config_dir}/config.toml}"
service_path="${PEDALCAST_SERVICE_PATH:-/etc/systemd/system/pedalcast.service}"
config_source="${PEDALCAST_CONFIG_SOURCE:-}"

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

if [[ -n "${config_source}" && ! -f "${config_source}" ]]; then
  echo "error: config source not found: ${config_source}" >&2
  exit 1
fi

cd "${repo_root}"
cargo build --release

sudo install -m 0755 "${repo_root}/target/release/pedalcast" "${binary_path}"
sudo mkdir -p "${config_dir}"

if [[ -f "${config_path}" && -z "${PEDALCAST_OVERWRITE_CONFIG:-}" ]]; then
  echo "Keeping existing config: ${config_path}"
  echo "Set PEDALCAST_OVERWRITE_CONFIG=1 to replace it."
else
  if [[ -n "${config_source}" ]]; then
    sudo install -m 0644 "${config_source}" "${config_path}"
  else
    config_tmp="$(mktemp)"
    mapfile -t adapters < <(find /sys/class/bluetooth -maxdepth 1 -type l -name 'hci*' -printf '%f\n' | sort -V)
    if [[ "${#adapters[@]}" -eq 0 ]]; then
      echo "error: no hci Bluetooth adapters found under /sys/class/bluetooth" >&2
      rm -f "${config_tmp}"
      exit 1
    fi

    server_adapter="${adapters[0]}"
    bike_adapter="${adapters[0]}"
    if [[ "${#adapters[@]}" -gt 1 ]]; then
      bike_adapter="${adapters[1]}"
    else
      echo "Only one Bluetooth adapter found (${server_adapter}); enabling single-adapter mode."
    fi

    cat >"${config_tmp}" <<EOF
[bike]
type = "keiser_m3i"
adapter = "${bike_adapter}"

[server]
type = "ble"
adapter = "${server_adapter}"
name = "Pedalcast"

[timeouts]
telemetry_stale_ms = 3000
bike_disconnect_ms = 300000

[filter]
suppress_single_zero_dropouts = true
EOF
    sudo install -m 0644 "${config_tmp}" "${config_path}"
    rm -f "${config_tmp}"
  fi
fi

sudo install -m 0644 "${repo_root}/deploy/pedalcast.service" "${service_path}"
sudo systemctl daemon-reload
sudo systemctl enable pedalcast.service
sudo systemctl restart pedalcast.service

echo "Pedalcast installed and started."
systemctl --no-pager --full status pedalcast.service
