#!/usr/bin/env bash
# Godotクライアントのテストをヘッドレスで実行する。
#
# 前提が3つあり、いずれも守らないと結果が信用できなくなる。
#
# 1. グローバルクラス名（class_name）の解決にインポート済みプロジェクトが要る。
#    frontend/.godot はGit管理外なので、新規チェックアウトでは必ずインポートから始める。
#
# 2. 合否はSceneTreeのquit()がそのままプロセス終了コードになるので、それで判定する。
#    出力中の "ERROR:" では判定しない。Godotは終了時に
#    「N resources still in use at exit」を出すことがあり、成功したテストまで落ちる。
#    ただしGDScriptの実行時エラーはquit()へ到達せずSceneTreeが回り続けるため、
#    テストごとにタイムアウトを掛ける。
#
# 3. 一部のテストはGameServerを必要とする。必要とする形が2通りある。
#    - room_flow_test:      アプリ自身が CREATE ROOM でバイナリを起動する
#    - join_room_flow_test: あらかじめ別プロセスが待ち受けている必要がある
#    後者の待受ポートはテスト内に直書きされているだけだったので、ここで面倒を見る。
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
godot_bin="${GODOT_BIN:-godot}"
game_server_bin="${GAME_SERVER_BIN:-${repository_root}/target/debug/pixel-shooter-server}"
test_timeout="${FRONTEND_TEST_TIMEOUT:-120}"
# join_room_flow_test.gd の SERVER_URL と揃える。
manual_server_port="${FRONTEND_MANUAL_SERVER_PORT:-9019}"
manual_control_port="${FRONTEND_MANUAL_CONTROL_PORT:-9119}"
# GameServerのバイナリが無いと成立しないテスト。
server_tests=("join_room_flow_test" "room_flow_test")

manual_pid=""

stop_manual_server() {
  if [[ -n "${manual_pid}" ]] && kill -0 "${manual_pid}" 2>/dev/null; then
    kill "${manual_pid}" 2>/dev/null || true
    wait "${manual_pid}" 2>/dev/null || true
  fi
  manual_pid=""
}
trap stop_manual_server EXIT

cd "${repository_root}"

"${godot_bin}" --headless --path frontend --import >/dev/null 2>&1 || true

has_server=0
if [[ -x "${game_server_bin}" ]]; then
  has_server=1
  PIXEL_SHOOTER_BIND_ADDR="127.0.0.1:${manual_server_port}" \
  PIXEL_SHOOTER_CONTROL_BIND_ADDR="127.0.0.1:${manual_control_port}" \
    "${game_server_bin}" >/dev/null 2>&1 &
  manual_pid=$!
  waited=0
  until curl --silent --fail "http://127.0.0.1:${manual_control_port}/internal/health" >/dev/null 2>&1; do
    waited=$((waited + 1))
    if [[ "${waited}" -ge 30 ]]; then
      echo "待受サーバー（:${manual_server_port}）が起動しなかった" >&2
      exit 1
    fi
    sleep 1
  done
fi

failed=0
skipped=""
for path in frontend/tests/*_test.gd; do
  name="$(basename "${path}" .gd)"

  if [[ "${has_server}" -eq 0 ]] && [[ " ${server_tests[*]} " == *" ${name} "* ]]; then
    skipped="${skipped} ${name}"
    printf '  SKIP %s\n' "${name}"
    continue
  fi

  set +e
  output="$(timeout "${test_timeout}" \
    "${godot_bin}" --headless --path frontend --script "res://tests/${name}.gd" 2>&1)"
  status=$?
  set -e

  if [[ "${status}" -eq 124 ]]; then
    printf '  FAIL %s（%s秒でタイムアウト。quit()へ到達していない）\n' "${name}" "${test_timeout}"
    printf '%s\n' "${output}" | sed 's/^/    /'
    failed=1
  elif [[ "${status}" -ne 0 ]] || printf '%s' "${output}" | grep -q "SCRIPT ERROR"; then
    printf '  FAIL %s\n' "${name}"
    printf '%s\n' "${output}" | sed 's/^/    /'
    failed=1
  else
    printf '  ok   %s\n' "${name}"
  fi
done

if [[ -n "${skipped}" ]]; then
  printf '\n未実行:%s\n' "${skipped}"
  printf 'GameServerのバイナリが要る。先に make build-game-server を実行するか、\n'
  printf 'make test-frontend-full を使って未実行を残さないこと。\n'
fi

exit "${failed}"
