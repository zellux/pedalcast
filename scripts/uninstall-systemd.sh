#!/usr/bin/env bash
set -euo pipefail

service_path="${PEDALCAST_SERVICE_PATH:-/etc/systemd/system/pedalcast.service}"
binary_path="${PEDALCAST_BINARY_PATH:-/usr/local/bin/pedalcast}"

sudo systemctl disable --now pedalcast.service 2>/dev/null || true
sudo rm -f "${service_path}"
sudo rm -f "${binary_path}"
sudo systemctl daemon-reload

echo "Pedalcast service and binary removed."
echo "Config is left in /etc/pedalcast; remove it manually if you no longer need it."
