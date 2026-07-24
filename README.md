# Pixel Shooter PvP

GodotフロントエンドとRust + Bevy権威サーバーで動く、1対1の見下ろし型シューティングゲーム。

## 必要環境

- Rust 1.92以降（Bevy 0.18を使用）
- Godot 4.7以降

## 起動

最初にサーバーを起動する。

```sh
cargo run -p pixel-shooter-server
```

Godotで `front/project.godot` を開いて実行する。2人で確認する場合は、エディター設定の
「複数インスタンスを実行」を有効にするか、ビルドしたクライアントを2つ起動する。

操作:

- WASD: 移動
- マウス: 照準
- 左クリック: 射撃
- R: リロード
- Space: 移動入力方向へダッシュ

クライアントは自分の移動を入力予測し、他プレイヤーは受信した位置の間を補間する。
サーバーの確定位置と差が出た場合は、未処理入力を再適用して滑らかに補正する。

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

ポートを変更した場合:

```sh
PIXEL_SHOOTER_SERVER_URL=ws://127.0.0.1:9002 node scripts/network_test.mjs
```

この模擬はクライアントからの入力と、サーバーからのスナップショットに適用される。
`PIXEL_SHOOTER_LATENCY_MS`は片道の遅延値である。参加処理を検証可能に保つため、
`join`、`welcome`、`rejected`には適用しない。

## 構成

- `front/`: Godotクライアント
- `back/`: Bevyヘッドレスサーバー
- `protocol/`: Rustの通信メッセージ型
- `docs/`: プロトコルとゲームルール
