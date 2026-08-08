SHELL := /bin/sh

CARGO ?= cargo
COMPOSE ?= docker compose
COMPOSE_RELEASE ?= docker compose -f docker-compose.release.yml
CURL ?= curl
GODOT_BIN ?= godot
NODE ?= node
NPM ?= npm
SSH ?= ssh

SERVICE ?=
# 既定はlinux。macOSはプロジェクト設定が足りず現状書き出せず、
# windowsはサーバーがホスト向けのまま同梱されるため、確実に通るものを既定にする。
RELEASE_TARGET ?= linux
GAME_SERVER_BINARY ?= target/debug/pixel-shooter-server
# 実行時エラーでquit()へ到達しないGodotテストを打ち切る。実測で最長は約10秒。
FRONTEND_TEST_TIMEOUT ?= 120
# 統合試験が使う待受ポート。Compose環境と併用する場合はここを変える。
INTEGRATION_TEST_PORT ?= 9001
INTEGRATION_TEST_CONTROL_PORT ?= 9101
SSH_HOST ?=
WAIT_SECONDS ?= 30

.PHONY: help doctor setup \
	dev up rebuild rebuild-release build-images config stop down restart reload-maps ps logs wait \
	integration integration-server urls tunnel \
	build build-game-server check test test-frontend test-frontend-full update-goldens fmt fmt-check lint verify \
	run-game-server run-matchmaker run-admin-server \
	web-install web-build web-check web-dist-check web-dev \
	godot sfx release

##@ 基本

help: ## 利用できる開発コマンドを表示
	@awk 'BEGIN {FS = ":.*## "; printf "Pixel Shooter PvP development commands\n"} /^##@ / {printf "\n%s\n", substr($$0, 5); next} /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-18s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
	@printf '\n使用例:\n'
	@printf '  make dev\n'
	@printf '  make logs SERVICE=matchmaker\n'
	@printf '  make tunnel SSH_HOST=backend-host\n'
	@printf '  make release RELEASE_TARGET=linux\n'

##@ 初期設定

doctor: ## 必要な開発ツールとバージョンを確認
	@missing=0; \
	for command in "$(CARGO)" docker "$(NODE)" "$(NPM)" "$(CURL)" "$(SSH)"; do \
		if ! command -v "$$command" >/dev/null 2>&1; then \
			printf 'missing: %s\n' "$$command"; \
			missing=1; \
		fi; \
	done; \
	if [ "$$missing" -ne 0 ]; then exit 1; fi
	@$(CARGO) --version
	@$(COMPOSE) version
	@$(NODE) --version
	@$(NPM) --version

setup: ## Rust依存とデバッグWeb依存を取得
	$(CARGO) fetch --locked
	$(NPM) --prefix tools/debug-web ci

##@ バックエンド（Docker Compose）

dev: ## 開発用共有イメージを再ビルド・起動し、利用可能になるまで待機
	$(COMPOSE) build admin-server
	$(COMPOSE) up --detach --no-build
	@$(MAKE) wait
	@$(MAKE) urls

up: ## ビルド済みComposeイメージを起動
	$(COMPOSE) up --detach --no-build $(SERVICE)

rebuild: ## 開発用共有イメージを1回だけ再ビルドして起動
	$(COMPOSE) build admin-server
	$(COMPOSE) up --detach --no-build $(SERVICE)

rebuild-release: ## 本番相当のreleaseイメージを再ビルドして起動
	$(COMPOSE_RELEASE) up --detach --build $(SERVICE)

build-images: ## 開発用共有イメージだけをビルド
	$(COMPOSE) build admin-server

config: ## 開発用・release用Compose設定を検証
	$(COMPOSE) config --quiet
	$(COMPOSE_RELEASE) config --quiet

stop: ## Composeサービスを停止（SERVICEで対象を限定可能）
	$(COMPOSE) stop $(SERVICE)

down: ## Composeサービスを停止してコンテナを削除
	$(COMPOSE) down

restart: ## Composeサービスを再起動（イメージは再ビルドしない）
	$(COMPOSE) restart $(SERVICE)

reload-maps: ## マップを再読込するためGame Serverだけを再起動
	$(COMPOSE) restart game-server-1 game-server-2

ps: ## Composeサービスの状態を表示
	$(COMPOSE) ps

logs: ## Composeログを追跡（例: make logs SERVICE=matchmaker）
	$(COMPOSE) logs --follow --tail=100 $(SERVICE)

wait: ## Matchmaker、Admin Server、Game Serverの準備完了を待機
	@printf 'Waiting for the backend'
	@remaining="$(WAIT_SECONDS)"; \
	until \
		$(CURL) --silent --fail http://127.0.0.1:8080/health >/dev/null 2>&1 && \
		$(CURL) --silent --fail http://127.0.0.1:8081/internal/health >/dev/null 2>&1 && \
		$(CURL) --silent --fail http://127.0.0.1:8081/api/servers 2>/dev/null | grep -q game-server-1 && \
		$(CURL) --silent --fail http://127.0.0.1:8081/api/servers 2>/dev/null | grep -q game-server-2; \
	do \
		remaining=$$((remaining - 1)); \
		if [ "$$remaining" -le 0 ]; then \
			printf '\nBackend did not become ready within %s seconds.\n' "$(WAIT_SECONDS)" >&2; \
			$(COMPOSE) ps >&2; \
			exit 1; \
		fi; \
		printf '.'; \
		sleep 1; \
	done
	@printf ' ready\n'

integration: wait ## 起動中のCompose環境に対して制御面の統合試験を実行
	$(NODE) scripts/control_plane_test.mjs

# 各試験は状態を持つため、1本ごとにサーバーを作り直す必要がある。
# 前提はスクリプト側にまとめてある。
integration-server: ## 単体GameServerに対する統合試験（要 make build-game-server）
	NODE="$(NODE)" GAME_SERVER_BIN="$(GAME_SERVER_BINARY)" \
		PIXEL_SHOOTER_TEST_PORT="$(INTEGRATION_TEST_PORT)" \
		PIXEL_SHOOTER_TEST_CONTROL_PORT="$(INTEGRATION_TEST_CONTROL_PORT)" \
		scripts/run_server_integration_tests.sh

urls: ## ローカル開発用URLを表示
	@printf 'Matchmaker:  http://127.0.0.1:8080\n'
	@printf 'Admin debug: http://127.0.0.1:8081/debug/\n'
	@printf 'GameServer 1: ws://127.0.0.1:9001\n'
	@printf 'GameServer 2: ws://127.0.0.1:9002\n'

tunnel: ## MacからバックエンドへSSHトンネルを作成（SSH_HOST必須）
	@if [ -z "$(SSH_HOST)" ]; then \
		printf 'Usage: make tunnel SSH_HOST=backend-host\n' >&2; \
		exit 2; \
	fi
	$(SSH) -N \
		-o ExitOnForwardFailure=yes \
		-o ServerAliveInterval=30 \
		-o ServerAliveCountMax=3 \
		-L 8080:127.0.0.1:8080 \
		-L 8081:127.0.0.1:8081 \
		-L 9001:127.0.0.1:9001 \
		-L 9002:127.0.0.1:9002 \
		"$(SSH_HOST)"

##@ Rust開発

check: ## Rust Workspaceを高速チェック
	$(CARGO) check --workspace --locked

build: ## Rust Workspace全体を開発プロファイルでビルド
	$(CARGO) build --workspace --locked

build-game-server: ## CREATE ROOM用のGame Serverをビルド
	$(CARGO) build --locked -p pixel-shooter-server

test: ## Rust Workspaceの全テストを実行
	$(CARGO) test --workspace --locked

# サーバー側の規則を変えたら、クライアント予測と通信フォーマットのゴールデンを
# 生成し直す。更新後は必ず test-frontend を通し、Godot側を追従させること。
update-goldens: ## クライアント契約テストの期待値を再生成
	UPDATE_MOVEMENT_GOLDEN=1 $(CARGO) test --locked \
		-p pixel-shooter-game-core --test movement_prediction_golden
	UPDATE_WIRE_GOLDEN=1 $(CARGO) test --locked \
		-p pixel-shooter-protocol --test wire_golden

# 前提（インポート、待受サーバー、合否判定、タイムアウト）はスクリプト側にまとめてある。
test-frontend: ## Godotクライアントのテストを実行（要GODOT_BIN）
	@GODOT_BIN="$(GODOT_BIN)" GAME_SERVER_BIN="$(GAME_SERVER_BINARY)" \
		FRONTEND_TEST_TIMEOUT="$(FRONTEND_TEST_TIMEOUT)" \
		scripts/run_frontend_tests.sh

test-frontend-full: build-game-server test-frontend ## Godotテストをローカルサーバー込みで実行

fmt: ## Rustコードを整形
	$(CARGO) fmt --all

fmt-check: ## Rustコードの整形を変更せず検査
	$(CARGO) fmt --all -- --check

lint: ## Clippyを警告エラー扱いで実行
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings

verify: fmt-check lint test web-check test-frontend ## コミット前の全検査を実行

run-game-server: ## Game Server単体を直接起動
	$(CARGO) run -p pixel-shooter-server

run-matchmaker: ## Matchmaker単体を直接起動
	$(CARGO) run -p pixel-shooter-matchmaker

run-admin-server: ## Admin Server単体を直接起動
	$(CARGO) run -p pixel-shooter-admin-server

##@ 管理Web

web-install: ## Adminデバッグ画面のnpm依存を取得
	$(NPM) --prefix tools/debug-web ci

web-build: ## Adminデバッグ画面をビルド
	$(NPM) --prefix tools/debug-web run build

web-check: ## Adminデバッグ画面を型検査
	$(NPM) --prefix tools/debug-web run check

# tools/debug-web/dist はコミット済みで、AdminServerが include_bytes! で埋め込む。
# srcを直してビルドを忘れると、配布されるデバッグ画面だけが古いまま残る。
# Viteのビルドは再現可能なので、差分の有無で陳腐化を判定できる。
web-dist-check: ## コミット済みのデバッグ画面が最新のsrcから作られているか確認
	@$(MAKE) --no-print-directory web-build
	@if ! git diff --quiet -- tools/debug-web/dist; then \
		printf 'tools/debug-web/dist が src と一致しない。\n'; \
		printf 'make web-build の結果をコミットすること。\n'; \
		git --no-pager diff --stat -- tools/debug-web/dist; \
		exit 1; \
	fi
	@printf 'tools/debug-web/dist は src と一致している。\n'

web-dev: ## Adminデバッグ画面のVite開発サーバーを起動
	$(NPM) --prefix tools/debug-web run dev

##@ Godot・アセット

godot: ## Godotエディターでfrontendを開く
	"$(GODOT_BIN)" --editor --path frontend


sfx: ## 効果音を再生成
	$(NODE) scripts/generate_sfx.mjs

##@ リリース

release: ## 配布物を作成（RELEASE_TARGET=linux|windows|macos|pck|server、既定はlinux）
	GODOT_BIN="$(GODOT_BIN)" ./scripts/build_release.sh "$(RELEASE_TARGET)"
