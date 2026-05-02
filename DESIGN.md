# Pedalcast Design

Pedalcast is a small service that translates proprietary indoor-bike telemetry
into standard training-app signals. The first target is a Keiser M3i bike
feeding Bluetooth LE cycling data to apps such as Zwift.

The core design goal is boring reliability: a long-running daemon on a Raspberry
Pi should survive flaky BLE advertisements, app reconnects, adapter quirks, and
short signal gaps without needing manual babysitting.

## Background

The current Gymnasticon setup proved the most important operational lesson:
Keiser M3i data arrives as BLE advertisements, not as a conventional paired
peripheral connection. If the same Bluetooth adapter is used for both scanning
the bike and advertising a GATT server to apps, the adapter can miss bike
advertisements while it is busy serving the app. The visible symptom is short
dropouts every minute or two, followed by automatic scan recovery.

The working production layout separates the two Bluetooth roles:

```toml
[adapters]
bike = 1
server = 0
```

`hci1` is the USB Bluetooth adapter dedicated to scanning the Keiser bike.
`hci0` is the onboard Bluetooth adapter dedicated to the app-facing server.

Pedalcast should treat adapter separation as a first-class design constraint,
not as a hidden workaround.

## Goals

- Provide a stable Keiser M3i to Bluetooth Cycling Power bridge.
- Run as a Linux daemon on Raspberry Pi OS using BlueZ.
- Bind scan and server roles to explicit Bluetooth adapter IDs.
- Emit useful structured logs for bike scan state, app connection state, and
  telemetry gaps.
- Prefer graceful degradation over noisy reconnect loops.
- Make the core data model independent from Keiser and BLE output details.
- Be easy to install, inspect, restart, and diagnose over SSH.

## Non-Goals

- Replacing full training platforms such as Zwift.
- Pairing with the Keiser bike as a connected BLE peripheral.
- Supporting every bike type in the first version.
- Building a mobile app or graphical dashboard initially.
- Depending on legacy Node BLE libraries or native addon behavior.

## Recommended Stack

Rust is the preferred implementation language.

Reasons:

- Good fit for a small always-on daemon.
- Strong ownership and error handling for long-running state machines.
- Single binary deployment.
- Efficient CPU and memory use on Raspberry Pi hardware.
- Better long-term maintainability than the older Node noble/bleno stack.

The BLE layer should use BlueZ through D-Bus where practical. Avoid a design that
depends on a high-level BLE wrapper if it hides adapter binding, scan lifecycle,
GATT registration, or notification errors.

## Architecture

Pedalcast has five main modules:

```text
Keiser scanner -> Telemetry normalizer -> Output services -> App clients
                         |
                         v
                 Supervisor / logs
```

### 1. Bike Input

The first input implementation is `keiser_m3i`.

Responsibilities:

- Bind to the configured bike adapter.
- Start passive BLE scanning.
- Filter advertisements by Keiser local name and payload shape.
- Decode manufacturer data into raw power and cadence.
- Track the bike address once discovered.
- Report scan lifecycle events separately from telemetry events.

Important behavior:

- The scanner must not treat every short gap as a disconnect.
- Missing advertisements should produce a telemetry-gap event with duration.
- A long gap should transition the bike state to stale/disconnected.
- Scan restarts should be explicit, rate-limited, and logged.

### 2. Telemetry Normalizer

The normalizer owns the app-independent data model:

```text
timestamp
power_watts
cadence_rpm
crank_revolutions
crank_event_time
source_quality
```

It should also handle Keiser-specific quirks:

- Preserve realistic zero power when the rider stops.
- Suppress obvious one-sample zero dropouts when cadence/power immediately
  recover.
- Keep dropout filtering visible in logs and counters.

The normalizer should produce a steady stream of measurements for output
services, but it should not invent aggressive synthetic power. When data is
stale, mark it stale instead of pretending the bike is still live.

### 3. Output Services

The first output is a BLE GATT server exposing standard cycling services.

Likely services:

- Cycling Power Service
- Cycling Speed and Cadence Service
- Fitness Machine Service, if needed by target apps

Responsibilities:

- Bind to the configured server adapter.
- Register GATT services with BlueZ.
- Advertise a stable device name, default `Pedalcast`.
- Notify subscribed app clients when new measurements arrive.
- Continue advertising after app disconnects.
- Log app connect/disconnect/subscription events.

The output path should be independent from Keiser. Future bike inputs should be
able to feed the same output service without changing GATT code.

### 4. Supervisor

The supervisor coordinates lifecycle and health:

- Validate adapter configuration on startup.
- Refuse to use one adapter for both roles unless explicitly allowed.
- Start bike scanner before output server, then keep both supervised.
- Restart scan or advertising components without restarting the whole daemon
  when possible.
- Expose useful process exit codes for systemd.

The supervisor should track separate states:

```text
bike_adapter: ready | missing | blocked | failed
server_adapter: ready | missing | blocked | failed
bike: searching | live | stale | disconnected
app_server: advertising | connected | failed
```

### 5. Observability

Logs should answer the questions that mattered during the real incident:

- Which HCI adapter is scanning the bike?
- Which HCI adapter is advertising to apps?
- Has the bike been discovered?
- Are stats arriving continuously?
- Did the app connect and subscribe to notifications?
- Are telemetry zeroes from the bike or from local timeout handling?
- Did BlueZ reject advertising or GATT registration?

Use structured logs with concise human-readable messages. Examples:

```text
INFO adapter.bike selected hci1 address=00:1A:7D:DA:71:15
INFO adapter.server selected hci0 address=B8:27:EB:59:75:67
INFO bike.keiser discovered address=f9:41:53:ad:6f:31
WARN bike.keiser telemetry_gap duration_ms=2200 action=scan_restart
INFO app.ble connected address=...
INFO app.ble subscribed service=cycling_power
```

## Configuration

Use TOML for the daemon config:

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

Adapter values should be numeric BlueZ/HCI indexes. The implementation may
accept `hci1` as a convenience, but it must normalize that to `1` internally and
log the resolved adapter.

## Systemd

The daemon should install as `pedalcast.service`.

Suggested runtime shape:

```ini
[Unit]
Description=Pedalcast
After=bluetooth.target
Requires=bluetooth.target

[Service]
Type=simple
ExecStart=/usr/local/bin/pedalcast --config /etc/pedalcast/config.toml
Restart=always
RestartSec=2
AmbientCapabilities=CAP_NET_RAW CAP_NET_ADMIN
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

## Data Flow

```text
Keiser BLE advertisement
  -> BlueZ scan event on bike adapter
  -> Keiser parser
  -> normalized telemetry event
  -> dropout/staleness filter
  -> GATT measurement encoder
  -> BLE notification on server adapter
  -> training app
```

## Failure Handling

### Bike Adapter Missing

Fail startup with a clear error:

```text
bike adapter hci1 not found; available adapters: hci0
```

### Server Adapter Missing

Fail startup. Running without an app-facing adapter is not useful for the first
version.

### Bike Stops Advertising

Do not immediately exit. Mark telemetry stale, optionally send zero power after
the stale threshold, and keep scanning. Log at warning level only when the gap
crosses a threshold.

### App Disconnects

Continue scanning the bike and keep the latest state. Restart advertising if
BlueZ stops it.

### BlueZ Registration Failure

Log the exact BlueZ D-Bus error and exit non-zero. This path should be easy to
diagnose from `journalctl -u pedalcast`.

## First Milestone

The first useful milestone is intentionally narrow:

- Rust daemon starts under systemd.
- Reads `/etc/pedalcast/config.toml`.
- Validates two Bluetooth adapters.
- Scans Keiser M3i advertisements on adapter `1`.
- Logs decoded power and cadence.
- Exposes Cycling Power over BLE on adapter `0`.
- A training app can discover `Pedalcast` and receive live power updates.

## Migration Plan

1. Keep the current Gymnasticon deployment as the known-good fallback.
2. Build Pedalcast read-only scanner first and compare logs against Gymnasticon.
3. Add BLE output and test with a secondary app profile.
4. Run Pedalcast for a short ride while keeping rollback instructions handy.
5. Replace Gymnasticon systemd service only after a full ride without dropouts.

## Open Questions

- Which training apps must be supported first: Zwift only, or also Wahoo,
  TrainerRoad, Apple Fitness, and others?
- Is Cycling Power Service enough, or should FTMS be included in the first
  release?
- Should Pedalcast expose a tiny local HTTP health endpoint?
- Should configuration allow single-adapter mode for debugging, even if it is
  discouraged?
- Should dropout filtering be conservative by default or fully transparent by
  default?
