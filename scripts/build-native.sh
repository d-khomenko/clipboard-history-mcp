#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$HERE/bin"
swiftc -O "$HERE/native/pasteboard-types.swift" -o "$HERE/bin/pasteboard-types"
echo "Built $HERE/bin/pasteboard-types"
