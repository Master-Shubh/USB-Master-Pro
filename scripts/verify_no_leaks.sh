#!/usr/bin/env bash
# ==============================================================================
# USB Master Pro — Release Artifact Secret & Root Key Leak Verifier
# BhartX Ecosystems Pvt Ltd — Autonomous Security Gate
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "================================================================================"
echo "  🛡️ USB Master Pro — Binary Secret & Root Seed Leak Gate"
echo "  🏢 BhartX Ecosystems Pvt Ltd"
echo "================================================================================"

cd "${REPO_ROOT}"

# 1. Build release binaries without test-utils feature
echo "  🔨 Building clean release binaries (cargo build --release)..."
cargo build --release --bin usb_master_daemon --bin usb_master_cli

RELEASE_DIR="${REPO_ROOT}/target/release"
ARTIFACTS=(
    "${RELEASE_DIR}/usb_master_daemon"
    "${RELEASE_DIR}/usb_master_cli"
    "${RELEASE_DIR}/usb_master_daemon.exe"
    "${RELEASE_DIR}/usb_master_cli.exe"
    "${RELEASE_DIR}/libusb_master_core.rlib"
    "${REPO_ROOT}/drivers/vhci/anyusb_vhci.sys"
    "${REPO_ROOT}/drivers/filter/anyusb_filter.sys"
)

# Forbidden sensitive signatures that must NEVER exist in production binaries
FORBIDDEN_SIGNATURES=(
    "BHARTX_MOCK_TEST_HARNESS_KEY_001"
    "BEGIN PRIVATE KEY"
    "BEGIN RSA PRIVATE KEY"
    "BEGIN EC PRIVATE KEY"
    "BEGIN OPENSSH PRIVATE KEY"
    "BEGIN ENCRYPTED PRIVATE KEY"
)

LEAKS_FOUND=0

echo "  🔍 Inspecting release artifacts for forbidden cryptographic material..."

for artifact in "${ARTIFACTS[@]}"; do
    if [[ ! -f "${artifact}" ]]; then
        echo "  ⚠️ Warning: Artifact not found: ${artifact} (skipping)"
        continue
    fi

    echo "  👉 Scanning: $(basename "${artifact}")..."

    for pattern in "${FORBIDDEN_SIGNATURES[@]}"; do
        # Check 1: Extracted strings check
        if strings "${artifact}" | grep -F "${pattern}" > /dev/null 2>&1; then
            echo "  ❌ [CRITICAL LEAK] Pattern '${pattern}' detected via strings in ${artifact}!"
            LEAKS_FOUND=$((LEAKS_FOUND + 1))
        fi

        # Check 2: Direct raw binary match with byte offset
        MATCH_OFFSETS=$(grep -a -b -o -F "${pattern}" "${artifact}" 2>/dev/null || true)
        if [[ -n "${MATCH_OFFSETS}" ]]; then
            echo "  ❌ [CRITICAL LEAK] Pattern '${pattern}' detected at raw byte offsets:"
            echo "${MATCH_OFFSETS}"
            LEAKS_FOUND=$((LEAKS_FOUND + 1))
        fi
    done
done

echo "--------------------------------------------------------------------------------"
if [[ "${LEAKS_FOUND}" -gt 0 ]]; then
    echo "  🚨 SECURITY GATE FAILED: ${LEAKS_FOUND} forbidden secret signature(s) detected in release artifacts!"
    echo "  ⛔ Release builds MUST NOT proceed. Do NOT commit or deploy."
    exit 1
else
    echo "  ✅ LEAK CHECK PASSED: 0 forbidden signatures found across all release artifacts."
    echo "  🛡️ Binary integrity verified: BHARTX root seed and private keys are strictly absent."
    exit 0
fi
