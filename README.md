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

## Raspberry Pi Install

Install Rust first if the Pi does not already have it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Install the Bluetooth tools, clone Pedalcast on the Pi, edit
[examples/config.toml](examples/config.toml) if your Bluetooth adapter numbers
differ, then install and start the service:

```sh
sudo apt install bluez
git clone <pedalcast-repo-url>
cd pedalcast
./scripts/install-systemd.sh
```

The installer builds `target/release/pedalcast`, installs it to
`/usr/local/bin/pedalcast`, installs config to `/etc/pedalcast/config.toml`, and
enables `pedalcast.service` at boot.

Useful service commands:

```sh
./scripts/status.sh
./scripts/logs.sh
sudo systemctl restart pedalcast
sudo systemctl stop pedalcast
```

To replace an existing `/etc/pedalcast/config.toml` with the example config:

```sh
PEDALCAST_OVERWRITE_CONFIG=1 ./scripts/install-systemd.sh
```

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
