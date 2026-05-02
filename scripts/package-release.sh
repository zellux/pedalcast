#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "${PEDALCAST_VERSION:-}" ]]; then
  version="${PEDALCAST_VERSION}"
elif command -v git >/dev/null 2>&1; then
  version="$(git -C "${repo_root}" describe --tags --always --dirty)"
else
  version="$(sed -n 's/^version = "\(.*\)"/v\1/p' "${repo_root}/Cargo.toml" | head -n 1)"
fi

if [[ -z "${version}" ]]; then
  echo "error: unable to determine package version" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME}/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  . "${HOME}/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Build the binary first or install Rust." >&2
  exit 1
fi

case "$(uname -m)" in
  armv6l | armv7l) arch="armv7" ;;
  aarch64) arch="aarch64" ;;
  x86_64) arch="x86_64" ;;
  *)
    echo "error: unsupported package architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

cd "${repo_root}"
cargo build --release

dist_dir="${repo_root}/dist"
package_dir="${dist_dir}/pedalcast-${version}-linux-${arch}"
rm -rf "${package_dir}"
mkdir -p "${package_dir}"
install -m 0755 "${repo_root}/target/release/pedalcast" "${package_dir}/pedalcast"
install -m 0644 "${repo_root}/README.md" "${package_dir}/README.md"
install -m 0644 "${repo_root}/examples/config.toml" "${package_dir}/config.toml"

tar -C "${package_dir}" -czf "${dist_dir}/pedalcast-linux-${arch}.tar.gz" pedalcast README.md config.toml
echo "${dist_dir}/pedalcast-linux-${arch}.tar.gz"
