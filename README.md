# Pedalcast

Pedalcast is a small Rust daemon for turning proprietary indoor-bike telemetry
into standard training-app signals. The first target is a Keiser M3i bike on one
Bluetooth adapter and an app-facing BLE server on another.

This repository currently contains:

- TOML config loading from `/etc/pedalcast/config.toml` or `--config`.
- Numeric and `hciN` adapter parsing.
- Linux adapter validation through `/sys/class/bluetooth`.
- Refusal to use one adapter for both bike scanning and app serving unless
  explicitly allowed.
- Structured startup and health logs.
- Keiser M3i manufacturer-data parsing and conservative single-zero dropout
  filtering.
- BlueZ advertising and a Cycling Power GATT service for training apps.
- Systemd installation scripts for Raspberry Pi style deployments.

## Bluetooth Adapters

Pedalcast can run with either one or two Bluetooth adapters:

- Two adapters are recommended. One adapter scans the bike while the other
  advertises the Cycling Power service to training apps, which is usually more
  stable.
- One adapter can work, and the installer will automatically create a
  single-adapter config when only one `hciN` device is present. Because scan and
  advertising share the same radio, some hardware will occasionally drop signal
  or reconnect.

## Raspberry Pi Install

Install the Bluetooth tools, download the release package, then install and
start the service. Rust is not required for the release package.

```sh
sudo apt install bluez
curl -fL https://github.com/zellux/pedalcast/releases/latest/download/pedalcast-linux-armv7.tar.gz -o pedalcast-linux-armv7.tar.gz
mkdir pedalcast
tar -xzf pedalcast-linux-armv7.tar.gz -C pedalcast
cd pedalcast
sudo ./scripts/install-systemd.sh
```

The installer installs `pedalcast` to `/usr/local/bin/pedalcast`, installs
config to `/etc/pedalcast/config.toml`, and enables `pedalcast.service` at boot.
On first install it detects available Bluetooth adapters and writes a config
automatically:

- Two or more adapters: `hci0` serves the app, `hci1` scans the bike.
- One adapter: the same adapter scans and advertises in single-adapter mode.

Single-adapter mode works on some controllers, but scan and advertising share
the same radio, so signal dropouts are more likely. For the most stable setup,
use two Bluetooth adapters.

Useful service commands:

```sh
./scripts/status.sh
./scripts/logs.sh
sudo systemctl restart pedalcast
sudo systemctl stop pedalcast
```

To replace an existing `/etc/pedalcast/config.toml` with the example config:

```sh
PEDALCAST_CONFIG_SOURCE=examples/config.toml PEDALCAST_OVERWRITE_CONFIG=1 ./scripts/install-systemd.sh
```

For a checked-in single-adapter example, see
[examples/config.single-adapter.toml](examples/config.single-adapter.toml).

To force installation from a specific local binary:

```sh
PEDALCAST_BINARY_SOURCE=/path/to/pedalcast ./scripts/install-systemd.sh
```

To force a source build even when a previous release binary exists in
`target/release`:

```sh
PEDALCAST_FORCE_BUILD=1 ./scripts/install-systemd.sh
```

To force using the GitHub release binary and skip local compilation:

```sh
PEDALCAST_NO_BUILD=1 ./scripts/install-systemd.sh
```

If an older Raspberry Pi OS cannot verify GitHub's TLS certificate, update CA
certificates first. As a last resort for an old local Pi image, you can skip TLS
certificate verification explicitly:

```sh
PEDALCAST_NO_BUILD=1 PEDALCAST_INSECURE_DOWNLOAD=1 ./scripts/install-systemd.sh
```

## Release Packaging

Build a GitHub release asset on a target machine:

```sh
./scripts/package-release.sh
```

On a 32-bit Raspberry Pi this writes `dist/pedalcast-linux-armv7.tar.gz`, which
`install-systemd.sh` can download and install without Rust.

To remove the service and installed binary:

```sh
./scripts/uninstall-systemd.sh
```

## Local Smoke Run

On a development machine without Bluetooth adapters exposed through Linux sysfs,
mock available adapters:

```sh
PEDALCAST_ADAPTERS=hci0,hci1 cargo run -- --config examples/config.toml --check
```

On the Raspberry Pi:

```sh
cargo run -- --config /etc/pedalcast/config.toml --check
```

## Configuration

See [examples/config.toml](examples/config.toml).
