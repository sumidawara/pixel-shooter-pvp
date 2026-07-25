#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
distribution_dir="${project_root}/dist"
godot_binary="${GODOT_BIN:-/Applications/Godot.app/Contents/MacOS/Godot}"
target_name="${1:-macos}"

mkdir -p "${distribution_dir}/server"

echo "[1/3] Building the Bevy server"
cargo build --release --manifest-path "${project_root}/Cargo.toml" -p pixel-shooter-server
cp "${project_root}/target/release/pixel-shooter-server" "${distribution_dir}/server/"
cp "${project_root}/server.json" "${distribution_dir}/server/"

if [[ "${target_name}" == "server" ]]; then
  echo "Server package: ${distribution_dir}/server"
  exit 0
fi

case "${target_name}" in
  macos)
    preset="macOS"
    output="${distribution_dir}/PixelShooterPvP.app"
    ;;
  windows)
    preset="Windows Desktop"
    output="${distribution_dir}/PixelShooterPvP.exe"
    ;;
  linux)
    preset="Linux"
    output="${distribution_dir}/PixelShooterPvP.x86_64"
    ;;
  pck)
    preset="macOS"
    output="${distribution_dir}/PixelShooterPvP.pck"
    ;;
  *)
    echo "Usage: $0 [macos|windows|linux|pck|server]" >&2
    exit 2
    ;;
esac

if [[ ! -x "${godot_binary}" ]]; then
  echo "Godot was not found at ${godot_binary}." >&2
  echo "Set GODOT_BIN to the Godot executable path." >&2
  exit 1
fi

echo "[2/3] Exporting the Godot client (${preset})"
if [[ "${target_name}" == "pck" ]]; then
  "${godot_binary}" --headless --path "${project_root}/front" --export-pack "${preset}" "${output}"
else
  "${godot_binary}" --headless --path "${project_root}/front" --export-release "${preset}" "${output}"
fi

echo "[3/3] Release output"
echo "${output}"
echo "${distribution_dir}/server"
