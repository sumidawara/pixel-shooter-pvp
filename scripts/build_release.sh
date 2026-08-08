#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
distribution_dir="${project_root}/dist"
godot_binary="${GODOT_BIN:-/Applications/Godot.app/Contents/MacOS/Godot}"
target_name="${1:-macos}"

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
  server)
    preset=""
    output=""
    ;;
  *)
    echo "Usage: $0 [macos|windows|linux|pck|server]" >&2
    exit 2
    ;;
esac

# Godotの確認はサーバーのビルドより先に済ませる。
# 後ろに置くと、1分かけてビルドしたあとで「Godotが無い」と言われる。
if [[ "${target_name}" != "server" ]]; then
  # PATH上のコマンド名でも絶対パスでも指定できるようにする。
  # `[[ -x godot ]]` はPATHを見ないため、コマンド名だと必ず失敗していた。
  resolved_godot="$(command -v "${godot_binary}" 2>/dev/null || true)"
  if [[ -z "${resolved_godot}" || ! -x "${resolved_godot}" ]]; then
    echo "Godot was not found: ${godot_binary}" >&2
    echo "PATHへ通すか、GODOT_BIN で実行ファイルを指定する。" >&2
    echo "  make release RELEASE_TARGET=${target_name} GODOT_BIN=/path/to/godot" >&2
    exit 1
  fi
  godot_binary="${resolved_godot}"

  # エクスポートテンプレートが無いと、Godotは警告を出しつつ
  # 中身の無いファイルを書き出してしまう。先に確かめる。
  godot_version="$("${godot_binary}" --version | head -1 | cut -d. -f1-3)"
  template_dir="${HOME}/.local/share/godot/export_templates/${godot_version}.stable"
  if [[ ! -d "${template_dir}" ]]; then
    echo "エクスポートテンプレートが見つからない: ${template_dir}" >&2
    echo "Godotの「エディター → エクスポートテンプレートの管理」から導入する。" >&2
    exit 1
  fi
fi

mkdir -p "${distribution_dir}/server"

echo "[1/3] Building the Bevy server"
cargo build --release --manifest-path "${project_root}/Cargo.toml" -p pixel-shooter-server
server_source="${project_root}/target/release/pixel-shooter-server"
server_name="pixel-shooter-server"
if [[ -f "${server_source}.exe" ]]; then
  server_source="${server_source}.exe"
  server_name="pixel-shooter-server.exe"
fi
cp "${server_source}" "${distribution_dir}/server/${server_name}"
cp "${project_root}/server.json" "${distribution_dir}/server/"

if [[ "${target_name}" == "server" ]]; then
  echo "Server package: ${distribution_dir}/server"
  exit 0
fi

echo "[2/3] Exporting the Godot client (${preset})"
if [[ "${target_name}" == "pck" ]]; then
  "${godot_binary}" --headless --path "${project_root}/frontend" --export-pack "${preset}" "${output}"
else
  "${godot_binary}" --headless --path "${project_root}/frontend" --export-release "${preset}" "${output}"
fi

if [[ "${target_name}" == "macos" ]]; then
  cp "${server_source}" "${output}/Contents/MacOS/pixel-shooter-server"
  cp "${project_root}/server.json" "${output}/Contents/Resources/server.json"
fi

echo "[3/3] Release output"
echo "${output}"
echo "${distribution_dir}/server"
