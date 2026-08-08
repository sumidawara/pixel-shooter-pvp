#!/usr/bin/env bash
# 単体GameServerに対する統合試験を、1本ごとにサーバーを作り直して実行する。
#
# これらの試験は状態を持つ。1台のサーバーを使い回すと、前の試験が残した
# プレイヤー・ルーム・試合フェーズのせいで後の試験が落ち、原因が分かりにくい
# 失敗になる（cpu_orphan_testは空のルームから始まる前提で書かれている）。
# 再接続猶予もテストごとに必要な値が違う。
#
# この前提はREADMEの散文にしか無かったため、ここへ移してCIから確実に回せるようにする。
#
#   scripts/run_server_integration_tests.sh
#   GAME_SERVER_BIN=target/release/pixel-shooter-server scripts/run_server_integration_tests.sh
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
game_server_bin="${GAME_SERVER_BIN:-${repository_root}/target/debug/pixel-shooter-server}"
node_bin="${NODE:-node}"
# server.json の control.bind_address と揃える。ここが生きていれば試験を開始できる。
health_url="${PIXEL_SHOOTER_HEALTH_URL:-http://127.0.0.1:9101/internal/health}"
startup_timeout_seconds="${STARTUP_TIMEOUT_SECONDS:-30}"

# 「試験名:再接続猶予（空なら既定値）」
# 猶予1秒は、途中離脱と孤立CPUの試験を現実的な時間で終わらせるために必要。
tests=(
  "network_test:"
  "reconnect_test:"
  "lobby_leave_test:"
  "forfeit_test:1"
  "cpu_orphan_test:1"
)

# 実行しない試験と、その理由。
#
# 黙って飛ばすと「全部通った」ように見えるため、毎回必ず表示する。
skipped=(
  "debug_web_test:現在のサーバーでは成立しない。/debug/api/health の read_only と\
 デバッグ画面の見出し Server Observer を要求するが、どちらも ff6c8e1 で失われた。\
 テストを現状へ合わせるか、サーバー側に戻すかは仕様の判断が要る。"
)

server_pid=""

stop_server() {
  if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  server_pid=""
}

trap stop_server EXIT

start_server() {
  local grace="$1"
  local log="$2"
  if [[ -n "${grace}" ]]; then
    PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS="${grace}" "${game_server_bin}" >"${log}" 2>&1 &
  else
    "${game_server_bin}" >"${log}" 2>&1 &
  fi
  server_pid=$!

  local waited=0
  until curl --silent --fail "${health_url}" >/dev/null 2>&1; do
    if ! kill -0 "${server_pid}" 2>/dev/null; then
      echo "  サーバーが起動直後に終了した" >&2
      cat "${log}" >&2
      return 1
    fi
    waited=$((waited + 1))
    if [[ "${waited}" -ge "${startup_timeout_seconds}" ]]; then
      echo "  サーバーが${startup_timeout_seconds}秒以内に応答しなかった" >&2
      cat "${log}" >&2
      return 1
    fi
    sleep 1
  done
}

if [[ ! -x "${game_server_bin}" ]]; then
  echo "GameServerのバイナリが無い: ${game_server_bin}" >&2
  echo "先に make build-game-server を実行するか、GAME_SERVER_BIN で指定する。" >&2
  exit 1
fi

log_dir="$(mktemp -d)"
trap 'stop_server; rm -rf "${log_dir}"' EXIT

failed=0
for entry in "${tests[@]}"; do
  name="${entry%%:*}"
  grace="${entry##*:}"
  log="${log_dir}/${name}.log"

  printf '%-22s grace=%-8s ' "${name}" "${grace:-default}"
  if ! start_server "${grace}" "${log}"; then
    failed=1
    stop_server
    continue
  fi

  if output="$("${node_bin}" "${repository_root}/scripts/${name}.mjs" 2>&1)"; then
    printf 'ok   %s\n' "$(printf '%s' "${output}" | tail -1 | cut -c1-100)"
  else
    printf 'FAIL\n'
    printf '%s\n' "${output}" | sed 's/^/    /'
    echo "    --- サーバーのログ ---"
    sed 's/^/    /' "${log}" | tail -20
    failed=1
  fi
  stop_server
done

for entry in "${skipped[@]}"; do
  printf '%-22s SKIP %s\n' "${entry%%:*}" "${entry#*:}"
done

if [[ "${failed}" -ne 0 ]]; then
  echo "統合試験に失敗がある。" >&2
  exit 1
fi
echo "単体GameServerの統合試験 ${#tests[@]} 本合格 / ${#skipped[@]} 本は未実行。"
