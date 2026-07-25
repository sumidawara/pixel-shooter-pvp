# Pixel Shooter PvP

GodotフロントエンドとRust + Bevy権威サーバーで動く、1対1の見下ろし型シューティングゲーム。

## 必要環境

- Rust 1.92以降（Bevy 0.18を使用）
- Godot 4.7以降

## 起動

最初にサーバーを起動する。

```sh
cargo run --release -p pixel-shooter-server
```

Godotで `front/project.godot` を開いて実行し、メニューでプレイヤー名と
`ws://127.0.0.1:9001` を入力して接続する。2人で確認する場合は、エディター設定の
「複数インスタンスを実行」を有効にするか、ビルドしたクライアントを2つ起動する。

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

- `front/`: Godotクライアント
  - `scenes/main.tscn`: メニューとゲーム画面を配置するルート
  - `scenes/game/`: Arena、Player、Bullet、GameScreen
  - `scenes/ui/`: 接続メニュー、HUD、プレイヤー状態表示
  - `scripts/network_client.gd`: AutoloadのWebSocketクライアント
  - `themes/pixel_theme.tres`: 共通ピクセルフォントとUIスタイル
- `back/`: Bevyヘッドレスサーバー
  - `src/main.rs`: 設定とSystemをBevy Appへ登録する起動処理
  - `src/config.rs`: `server.json`と環境変数の読み込み
  - `src/model.rs`: Player、Bullet、MatchState
  - `src/arena.rs`: マップ形状、衝突判定、スポーン地点
  - `src/game.rs`: 試合進行、移動、射撃、リスポーン
  - `src/network.rs`: WebSocket、接続管理、スナップショット配信
- `protocol/`: Rustの通信メッセージ型
- `docs/`: プロトコルとゲームルール
- `server.json`: サーバーの運用・ゲーム設定
- `scripts/build_release.sh`: サーバーとクライアントの配布ビルド

Godot側のシーン構成とデータの流れは
[`docs/frontend-architecture.md`](docs/frontend-architecture.md) を参照。
