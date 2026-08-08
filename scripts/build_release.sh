#!/usr/bin/env bash
#
# 配布物を作る。
#
# このゲームは「Godotクライアント」と「Rust製GameServer」の2つで動く。
# どちらか片方だけでは遊べないため、まとめて dist/ へ置く。
#
#   dist/
#   ├── PixelShooterPvP.x86_64   クライアント本体（Linuxの場合）
#   ├── PixelShooterPvP.pck      ゲームデータ
#   └── server/
#       ├── pixel-shooter-server GameServer
#       └── server.json          サーバー設定
#
# この配置には意味がある。クライアントの CREATE ROOM は、自分の実行ファイルと
# 同じ階層の server/ からGameServerを探して子プロセスとして起動する
# （frontend/src/networking/host_server_controller.gd）。
# バラバラに配ると、部屋を作れないクライアントが出来上がる。
#
# macOSだけは配置が違い、.app の中へ入れる。アプリを1つ配れば済むようにするため。
#
# 使い方:
#   ./scripts/build_release.sh [linux|windows|macos|pck|server]   （既定: linux）
#   make release RELEASE_TARGET=linux
#
# Godotが必要なのはクライアントを書き出すときだけ。server は要らない。
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
distribution_dir="${project_root}/dist"
# 既定はPATH上の godot。コマンド名でも絶対パスでも受け付ける（下で解決する）。
# macOSでアプリとして入れている場合は、PATHに無いので GODOT_BIN で指定する:
#   GODOT_BIN=/Applications/Godot.app/Contents/MacOS/Godot
godot_binary="${GODOT_BIN:-godot}"
# 既定はlinux。現状これだけが最後まで通る（macOSは設定不足で失敗し、
# windowsはホスト向けサーバーが同梱される）。
target_name="${1:-linux}"

# 書き出す対象を決める。preset は frontend/export_presets.cfg の名前と一致させる。
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
    # ゲームデータだけを固めて、中身が壊れていないかを見るための対象。
    # 実行ファイルを作らないのでエクスポートテンプレートが要らず、
    # テンプレートを入れていない環境でも確認できる。
    # preset はどれでもよいが、既定の対象に合わせて Linux を使う。
    preset="Linux"
    output="${distribution_dir}/PixelShooterPvP.pck"
    ;;
  server)
    # サーバーだけを配る場合。Godotを一切使わない。
    preset=""
    output=""
    ;;
  *)
    echo "Usage: $0 [linux|windows|macos|pck|server]" >&2
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
    echo "  macOSでアプリとして入れている場合:" >&2
    echo "    GODOT_BIN=/Applications/Godot.app/Contents/MacOS/Godot" >&2
    exit 1
  fi
  godot_binary="${resolved_godot}"

  # エクスポートテンプレートが無いと、Godotは警告を出しつつ
  # 中身の無いファイルを書き出してしまう。先に確かめる。
  #
  # pck は実行ファイルを作らないためテンプレートが要らない。
  # ここで一緒に弾くと、テンプレート未導入の環境で確認する手段が無くなる。
  if [[ "${target_name}" != "pck" ]]; then
    # `4.7.1.stable.official.xxxx` の先頭3つを取る。安定版以外は形が違う。
    godot_version="$("${godot_binary}" --version | head -1 | cut -d. -f1-3)"
    template_dir="${HOME}/.local/share/godot/export_templates/${godot_version}.stable"
    if [[ ! -d "${template_dir}" ]]; then
      echo "エクスポートテンプレートが見つからない: ${template_dir}" >&2
      echo "Godotの「エディター → エクスポートテンプレートの管理」から導入する。" >&2
      echo "テンプレート無しで中身だけ確認するなら RELEASE_TARGET=pck を使う。" >&2
      exit 1
    fi
  fi
fi

mkdir -p "${distribution_dir}/server"

echo "[1/3] Building the Bevy server"
# --locked を付けるのは、配布物を作るときこそ依存の版を動かしたくないため。
# Makefile と Dockerfile は既に付いており、ここだけ抜けていた。
# バイナリからデバッグシンボルを落とす設定は Cargo.toml の [profile.release] にある。
cargo build --release --locked --manifest-path "${project_root}/Cargo.toml" -p pixel-shooter-server
server_source="${project_root}/target/release/pixel-shooter-server"
server_name="pixel-shooter-server"
# Windows向けにビルドした場合は拡張子が付く。
if [[ -f "${server_source}.exe" ]]; then
  server_source="${server_source}.exe"
  server_name="pixel-shooter-server.exe"
fi
cp "${server_source}" "${distribution_dir}/server/${server_name}"
# server.json はサーバーの実行ファイルの隣に置く。
# サーバーはカレントディレクトリの次にここを見る（backend/game-server/src/config.rs）。
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

# macOSはアプリバンドルの中へサーバーを入れ、1つ配れば済むようにする。
# CREATE ROOM は .app の中では実行ファイルの隣（Contents/MacOS/）を見るため、
# dist/server/ とは別にここへも置く必要がある。
if [[ "${target_name}" == "macos" ]]; then
  cp "${server_source}" "${output}/Contents/MacOS/pixel-shooter-server"
  cp "${project_root}/server.json" "${output}/Contents/Resources/server.json"
fi

echo "[3/3] Release output"
echo "${output}"
echo "${distribution_dir}/server"
