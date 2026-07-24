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

## 構成

- `front/`: Godotクライアント
- `back/`: Bevyヘッドレスサーバー
- `protocol/`: Rustの通信メッセージ型
- `docs/`: プロトコルとゲームルール
