# Pedalcast

Pedalcast is a small Rust daemon for turning proprietary indoor-bike telemetry
into standard training-app signals. The first target is a Keiser M3i bike on one
Bluetooth adapter and an app-facing BLE server on another.

This repository currently contains the first daemon foundation:

- TOML config loading from `/etc/pedalcast/config.toml` or `--config`.
- Numeric and `hciN` adapter parsing.
- Linux adapter validation through `/sys/class/bluetooth`.
- Refusal to use one adapter for both bike scanning and app serving unless
  explicitly allowed.
- Structured startup and health logs.
- Keiser M3i manufacturer-data parsing and conservative single-zero dropout
  filtering.

The BlueZ D-Bus scanner and GATT server are the next implementation layer.

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

