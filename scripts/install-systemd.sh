#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

binary_path="${PEDALCAST_BINARY_PATH:-/usr/local/bin/pedalcast}"
config_dir="${PEDALCAST_CONFIG_DIR:-/etc/pedalcast}"
config_path="${PEDALCAST_CONFIG_PATH:-${config_dir}/config.toml}"
service_path="${PEDALCAST_SERVICE_PATH:-/etc/systemd/system/pedalcast.service}"
config_source="${PEDALCAST_CONFIG_SOURCE:-}"
release_base="${PEDALCAST_RELEASE_BASE:-https://github.com/zellux/pedalcast/releases/latest/download}"
binary_source="${PEDALCAST_BINARY_SOURCE:-}"

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME}/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  . "${HOME}/.cargo/env"
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

temp_dir=""
cleanup() {
  if [[ -n "${temp_dir}" ]]; then
    rm -rf "${temp_dir}"
  fi
}
trap cleanup EXIT

detect_release_arch() {
  case "$(uname -m)" in
    armv6l | armv7l) echo "armv7" ;;
    aarch64) echo "aarch64" ;;
    x86_64) echo "x86_64" ;;
    *)
      echo "error: unsupported release architecture: $(uname -m)" >&2
      return 1
      ;;
  esac
}

download_release_binary() {
  if ! command -v curl >/dev/null 2>&1; then
    echo "error: cargo not found and curl not found; cannot download a release binary." >&2
    return 1
  fi
  if ! command -v tar >/dev/null 2>&1; then
    echo "error: cargo not found and tar not found; cannot unpack a release binary." >&2
    return 1
  fi

  local arch
  arch="$(detect_release_arch)"
  local asset="pedalcast-linux-${arch}.tar.gz"
  local url="${release_base}/${asset}"
  temp_dir="$(mktemp -d)"
  echo "cargo not found; downloading ${url}"
  curl -fL "${url}" -o "${temp_dir}/${asset}"
  tar -xzf "${temp_dir}/${asset}" -C "${temp_dir}"
  if [[ ! -x "${temp_dir}/pedalcast" ]]; then
    echo "error: release asset did not contain an executable named pedalcast" >&2
    return 1
  fi
  binary_source="${temp_dir}/pedalcast"
}

if [[ -n "${binary_source}" ]]; then
  if [[ ! -x "${binary_source}" ]]; then
    echo "error: binary source is not executable: ${binary_source}" >&2
    exit 1
  fi
elif [[ -n "${PEDALCAST_NO_BUILD:-}" ]]; then
  download_release_binary
elif [[ -x "${repo_root}/target/release/pedalcast" && -z "${PEDALCAST_FORCE_BUILD:-}" ]]; then
  binary_source="${repo_root}/target/release/pedalcast"
elif command -v cargo >/dev/null 2>&1; then
  cargo build --release
  binary_source="${repo_root}/target/release/pedalcast"
else
  download_release_binary
fi

sudo install -m 0755 "${binary_source}" "${binary_path}"
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
