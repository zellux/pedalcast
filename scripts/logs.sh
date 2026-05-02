#!/usr/bin/env bash
set -euo pipefail

journalctl -u pedalcast.service -f
