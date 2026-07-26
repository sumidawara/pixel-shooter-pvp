# Pixel Shooter PvP

GodotフロントエンドとRust + Bevy権威サーバーで動く、最大4人の見下ろし型シューティングゲーム。
120秒で撃破とアイテム取得によるポイントを競い、死亡時には得点ペナルティが入る。

## 必要環境

- Rust 1.92以降（Bevy 0.18を使用）
- Godot 4.7以降

## 起動

最初にサーバーを起動する。

```sh
cargo run --release -p pixel-shooter-server
```

Godotで `frontend/project.godot` を開いて実行し、`PLAY`から`JOIN ROOM`を選んで
`ws://127.0.0.1:9001`へ接続する。最初に入室したプレイヤーがホストになり、
CPU追加、ルール設定、試合開始を操作できる。2人で確認する場合は、エディター設定の
「複数インスタンスを実行」を有効にするか、CPUを追加する。
ホスト1人のままSTART GAMEを押した場合は、対戦相手のCPUが1体自動追加される。

デスクトップ版の`CREATE ROOM`は同梱したRustサーバーを子プロセスとして起動する。
Godotエディターでは`target/debug/pixel-shooter-server`を自動検出するため、
先に一度`cargo build -p pixel-shooter-server`を実行しておく。
ターミナルでサーバーを先に起動した場合は`CREATE ROOM`ではなく`JOIN ROOM`を使う。
同じポートで`CREATE ROOM`を選ぶと、ポート使用中のエラーを表示する。

複数サーバー構成はDocker Composeで起動できる。

```sh
docker compose up --build
```

Godotの`JOIN ROOM`へ`http://127.0.0.1:8080`を入力すると、Matchmakerが
GameServerを割り当て、Join Ticketを発行し、そのGameServerへ直接接続する。
詳しくは[`docs/deployment.md`](docs/deployment.md)を参照。

Godotクライアントの接続先は
[`frontend/scripts/network_config.gd`](frontend/scripts/network_config.gd)へ集約している。
Docker Compose利用時の初期接続先はMatchmakerの`http://127.0.0.1:8080`。
一時的に別の接続先を使う場合は、起動時に`PIXEL_SHOOTER_SERVER_URL`環境変数でも
上書きできる。

AdminServerのデバッグ画面は`http://127.0.0.1:8081/debug/`で確認できる。
対象GameServerの選択、マップ・Entity・Snapshotの表示に加え、
Pause、1 tick Step、Resumeを操作できる。Svelte画面を変更した場合は、
Rustをビルドする前に次を実行して埋め込み用ファイルを更新する。

```sh
cd tools/debug-web
npm ci
npm run build
```

操作:

- WASD: 移動
- マウス: 照準
- 左クリック: 射撃
- R: リロード
- Space: 移動入力方向へダッシュ
- Esc: 接続を終了してメニューへ戻る

クライアントは自分の移動を入力予測し、他プレイヤーは受信した位置の間を補間する。
サーバーの確定位置と差が出た場合は、未処理入力を再適用して滑らかに補正する。

## サーバー設定

ゲームルールとネットワーク設定は [`server.json`](server.json) で変更できる。
設定ファイルの全項目と環境変数による上書きは
[`docs/server-settings.md`](docs/server-settings.md) を参照。

GameServerの`control`設定はAdminServerへの登録、内部API、Join Ticket検証に使う。
Control APIは外部へ直接公開せず、AdminServerを介して操作する。

## 遅延・パケット欠落試験

サーバーは環境変数で、スナップショットの片道遅延と欠落率を模擬できる。

```sh
PIXEL_SHOOTER_LATENCY_MS=120 \
PIXEL_SHOOTER_PACKET_LOSS_PERCENT=20 \
cargo run -p pixel-shooter-server
```

9001番ポートを別のサーバーが使用中なら、試験用ポートも変更できる。

```sh
PIXEL_SHOOTER_BIND_ADDR=127.0.0.1:9002 \
PIXEL_SHOOTER_LATENCY_MS=120 \
PIXEL_SHOOTER_PACKET_LOSS_PERCENT=20 \
cargo run -p pixel-shooter-server
```

別のターミナルで自動試験クライアントを実行する。

```sh
node scripts/network_test.mjs
```

切断・再接続・一時停止の試験:

```sh
node scripts/reconnect_test.mjs
```

途中離脱の短縮試験では、再接続猶予だけを1秒にできる。

```sh
PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS=1 cargo run -p pixel-shooter-server
node scripts/forfeit_test.mjs
node scripts/cpu_orphan_test.mjs
```

ポートを変更した場合:

```sh
PIXEL_SHOOTER_SERVER_URL=ws://127.0.0.1:9002 node scripts/network_test.mjs
```

この模擬はクライアントからの入力と、サーバーからのスナップショットに適用される。
`PIXEL_SHOOTER_LATENCY_MS`は片道の遅延値である。参加処理を検証可能に保つため、
`join`、`welcome`、`rejected`には適用しない。

## ビルドと配布

Godotの「エディター → エクスポートテンプレートの管理」から4.7用テンプレートを
インストールした後、macOS版とサーバーを次のコマンドで作成する。

```sh
./scripts/build_release.sh macos
```

`windows`、`linux`、`server` も指定できる。Godot本体が標準の場所にない場合は
`GODOT_BIN` で実行ファイルを指定する。出力はGit管理外の `dist/` に作られる。

エクスポートテンプレートなしでクライアントのパックだけを検証する場合:

```sh
./scripts/build_release.sh pck
```

画像・フォントの出典と効果音の再生成方法は
[`docs/assets.md`](docs/assets.md) に記載している。

## 構成

- `frontend/`: Godotクライアント
  - `scenes/main.tscn`: メニューとゲーム画面を配置するルート
  - `scripts/host_server_controller.gd`: デスクトップ版のRustサーバー起動・終了
  - `scenes/game/`: Arena、Player、Bullet、GameScreen
  - `scenes/ui/`: 接続メニュー、HUD、プレイヤー状態表示
  - `scripts/network_client.gd`: AutoloadのWebSocketクライアント
  - `themes/pixel_theme.tres`: 共通ピクセルフォントとUIスタイル
- `backend/game-core/`: 実時間と通信から独立した`GameTick`とゲームルール
- `backend/game-server/`: 1プロセス＝1ルームのWebSocket権威サーバー
- `backend/matchmaker/`: GameServer割当要求とJoin Ticket発行
- `backend/admin-server/`: サーバープール管理、Control中継、デバッグ画面
- `protocol/`: GodotとGameServer間のゲーム通信型
- `admin-protocol/`: サーバー間の管理通信型とTicket署名
- `docs/`: プロトコル、ゲームルール、バックエンド構成
- `docker-compose.yml`: 固定数GameServerプールの開発構成
- `server.json`: サーバーの運用・ゲーム設定
- `scripts/build_release.sh`: サーバーとクライアントの配布ビルド

Godot側のシーン構成とデータの流れは
[`docs/frontend-architecture.md`](docs/frontend-architecture.md) を参照。
バックエンドのtick実行フローは
[`docs/backend-architecture.md`](docs/backend-architecture.md) を参照。
