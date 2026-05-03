# Pedalcast

Pedalcast is a small Rust daemon for turning proprietary indoor-bike telemetry
into standard training-app signals. The first target is a Keiser M3i bike on one
Bluetooth adapter and an app-facing BLE server on another.

The intended deployment is a Raspberry Pi near the bike. Pedalcast passively
scans Keiser M3i BLE advertisements, normalizes the live power/cadence samples,
and exposes them to training apps as a standard Bluetooth Cycling Power service
named `Pedalcast`.

## What It Does

Pedalcast currently contains:

- TOML config loading from `/etc/pedalcast/config.toml` or `--config`.
- Numeric and `hciN` adapter parsing.
- Linux adapter validation through `/sys/class/bluetooth`.
- Refusal to use one adapter for both bike scanning and app serving unless
  explicitly allowed.
- Structured startup and health logs.
- Keiser M3i manufacturer-data parsing and conservative single-zero dropout
  filtering.
- BlueZ advertising and a Cycling Power GATT service for training apps.
- Systemd installation scripts for Raspberry Pi deployments.

## Easiest Raspberry Pi Install

This is the normal install path. It does not require Git, Rust, or Cargo on the
Pi because it uses the latest prebuilt release package.

```sh
sudo apt update
sudo apt install bluez curl tar
curl -fL https://github.com/zellux/pedalcast/releases/latest/download/pedalcast-linux-armv7.tar.gz -o pedalcast-linux-armv7.tar.gz
mkdir pedalcast
tar -xzf pedalcast-linux-armv7.tar.gz -C pedalcast
cd pedalcast
sudo ./scripts/install-systemd.sh
```

The installer:

- Installs the binary to `/usr/local/bin/pedalcast`.
- Installs config to `/etc/pedalcast/config.toml`.
- Installs and enables `pedalcast.service`.
- Starts the service immediately.
- Auto-detects available Bluetooth adapters and writes a first config.

Adapter defaults:

- Two or more adapters: `hci0` serves the app, `hci1` scans the bike.
- One adapter: the same adapter scans and advertises in single-adapter mode.

Two adapters are strongly preferred. A single adapter can work, but scanning for
the bike and advertising to the app share the same radio, so some controllers
will occasionally miss bike packets or reconnect.

## Service Checks

Useful commands on the Pi:

```sh
./scripts/status.sh
./scripts/logs.sh
sudo systemctl restart pedalcast
sudo systemctl stop pedalcast
```

Healthy startup should include lines like:

```text
config loaded path=/etc/pedalcast/config.toml bike_adapter=hci1 server_adapter=hci0
app.gatt registered adapter=hci0 service=cycling_power
app.ble advertising_registered adapter=hci0 name=Pedalcast service=cycling_power
bike.keiser scan_started adapter=hci1
```

When the bike is awake and sending live data, logs should include:

```text
bike.keiser telemetry address=... power_watts=... cadence_rpm=... quality=Live
```

If the app cannot see `Pedalcast`, first check that `app.ble
advertising_registered` appears. If the app connects but shows no numbers, check
for `bike.keiser telemetry`.

## Configuration

See [examples/config.toml](examples/config.toml). A two-adapter config looks
like this:

```toml
[bike]
type = "keiser_m3i"
adapter = 1

[server]
type = "ble"
adapter = 0
name = "Pedalcast"

[timeouts]
telemetry_stale_ms = 3000
bike_disconnect_ms = 300000

[filter]
suppress_single_zero_dropouts = true
```

For a checked-in single-adapter example, see
[examples/config.single-adapter.toml](examples/config.single-adapter.toml).

To replace an existing `/etc/pedalcast/config.toml` with the two-adapter example:

```sh
PEDALCAST_CONFIG_SOURCE=examples/config.toml PEDALCAST_OVERWRITE_CONFIG=1 ./scripts/install-systemd.sh
```

## Keiser BLE Spec

The Keiser M3i input follows Keiser's official parser reference:

- [KeiserCorp/Keiser.MSeries.BLE-Parser](https://github.com/KeiserCorp/Keiser.MSeries.BLE-Parser)

Important parser notes:

- Keiser packets may arrive with the `02 01` manufacturer prefix, or with that
  prefix stripped by the Bluetooth stack.
- Build bytes are encoded oddly: the official parser converts a byte to a hex
  string, then parses that string as decimal. For example, `0x30` means build
  minor `30`.
- Build major `6` is the supported M-series format.
- Cadence and heart-rate values are broadcast with one decimal place and are
  truncated with integer `/ 10`, matching the official parser.
- Pedalcast parses non-realtime packets but does not forward them to training
  apps. Only live Keiser packets become Cycling Power measurements.

## Bluetooth Adapters

Pedalcast can run with either one or two Bluetooth adapters:

- Two adapters are recommended. One adapter scans the bike while the other
  advertises the Cycling Power service to training apps, which is usually more
  stable.
- One adapter can work, and the installer will automatically create a
  single-adapter config when only one `hciN` device is present. Because scan and
  advertising share the same radio, some hardware will occasionally drop signal
  or reconnect.

## Installer Overrides

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

## Development Notes

Normal Pi installs should use the release package above. This section is for
development from a source checkout.

On a development machine without Bluetooth adapters exposed through Linux sysfs,
mock available adapters:

```sh
PEDALCAST_ADAPTERS=hci0,hci1 cargo run -- --config examples/config.toml --check
```

On the Raspberry Pi:

```sh
cargo run -- --config /etc/pedalcast/config.toml --check
```

Run the test suite from a source checkout:

```sh
cargo test
```

On a Pi with Rust installed through `rustup`, load Cargo into the shell first:

```sh
. "$HOME/.cargo/env"
cargo test
cargo build --release
sudo install -m 0755 target/release/pedalcast /usr/local/bin/pedalcast
sudo systemctl restart pedalcast.service
```

Practical dev checklist:

- Keep source-of-truth config examples under `examples/`.
- `install-systemd.sh` is the durable install path for release packages and
  source builds.
- Prefer testing parser changes on the Pi with `cargo test` before replacing
  `/usr/local/bin/pedalcast`.
- After restarting the service, verify both the app-facing path
  (`app.ble advertising_registered`) and the bike-facing path
  (`bike.keiser telemetry`).
