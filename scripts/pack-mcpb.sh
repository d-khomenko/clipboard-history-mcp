#!/usr/bin/env bash
# pack-mcpb.sh — Build universal macOS binary and package as .mcpb
#
# Usage:
#   bash scripts/pack-mcpb.sh
#
# Prerequisites:
#   . "$HOME/.cargo/env"            # Rust toolchain in PATH
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
#
# Output: clipboard-history-mcp.mcpb (ZIP archive)
#
# Note: universal binary (aarch64 + x86_64) requires both Rust targets.
# If x86_64 cross-compilation is unavailable, set BUILD_ARCH=arm64 to
# produce an ARM64-only package.

set -euo pipefail

# Ensure Rust toolchain is in PATH
# shellcheck source=/dev/null
[[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BUILD_ARCH="${BUILD_ARCH:-universal}"
BINARY_NAME="clipboard-history-mcp"
OUT_MCPB="${REPO_ROOT}/${BINARY_NAME}.mcpb"

echo "==> pack-mcpb: repo=${REPO_ROOT} arch=${BUILD_ARCH}"

# ── 1. Build ────────────────────────────────────────────────────────────────

if [[ "${BUILD_ARCH}" == "universal" ]]; then
    echo "==> Building aarch64-apple-darwin ..."
    cargo build --release --target aarch64-apple-darwin

    echo "==> Building x86_64-apple-darwin ..."
    cargo build --release --target x86_64-apple-darwin

    echo "==> Creating universal binary with lipo ..."
    mkdir -p target/universal-apple-darwin
    lipo -create \
        target/aarch64-apple-darwin/release/${BINARY_NAME} \
        target/x86_64-apple-darwin/release/${BINARY_NAME} \
        -output target/universal-apple-darwin/${BINARY_NAME}

    BINARY_PATH="target/universal-apple-darwin/${BINARY_NAME}"
    echo "==> Universal binary: $(lipo -info ${BINARY_PATH})"

elif [[ "${BUILD_ARCH}" == "arm64" ]]; then
    echo "==> Building aarch64-apple-darwin only ..."
    cargo build --release --target aarch64-apple-darwin
    BINARY_PATH="target/aarch64-apple-darwin/release/${BINARY_NAME}"
    echo "==> ARM64-only build (x86_64 planned for v0.3.1)"

else
    echo "Error: BUILD_ARCH must be 'universal' or 'arm64'" >&2
    exit 1
fi

# ── 2. Assemble staging directory ───────────────────────────────────────────

STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "${STAGING_DIR}"' EXIT

echo "==> Staging in ${STAGING_DIR} ..."
mkdir -p "${STAGING_DIR}/server"

cp mcpb/manifest.json "${STAGING_DIR}/manifest.json"
cp "${BINARY_PATH}"   "${STAGING_DIR}/server/${BINARY_NAME}"
chmod +x              "${STAGING_DIR}/server/${BINARY_NAME}"

# Include vendored gitleaks rules alongside the binary
if [[ -f vendor/gitleaks.toml ]]; then
    cp vendor/gitleaks.toml "${STAGING_DIR}/server/gitleaks.toml"
fi

# ── 3. Pack ─────────────────────────────────────────────────────────────────

rm -f "${OUT_MCPB}"

echo "==> Packing ${OUT_MCPB} ..."
# Try npx @anthropic-ai/mcpb first; fall back to plain zip
if command -v npx &>/dev/null && npx -y @anthropic-ai/mcpb --version &>/dev/null 2>&1; then
    echo "   using @anthropic-ai/mcpb"
    # mcpb pack [directory] [output]
    npx -y @anthropic-ai/mcpb pack "${STAGING_DIR}" "${OUT_MCPB}"
else
    echo "   @anthropic-ai/mcpb unavailable — falling back to zip"
    (cd "${STAGING_DIR}" && zip -r "${OUT_MCPB}" .)
fi

# ── 4. Verify ────────────────────────────────────────────────────────────────

echo "==> Verifying ..."
ls -lh "${OUT_MCPB}"
unzip -l "${OUT_MCPB}" | head -15

echo ""
echo "Done! ${OUT_MCPB}"
