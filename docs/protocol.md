# 通信プロトコル

第1段階では `ws://127.0.0.1:9001` にJSONテキストを送受信する。
クライアントは位置やHPを送らず、入力だけを送る。サーバーがすべてのゲーム状態を決定する。

## クライアントからサーバー

接続直後:

```json
{"type":"join","name":"Player"}
```

入力（毎フレーム）:

```json
{
  "type":"input",
  "sequence":42,
  "move_x":1.0,
  "move_y":0.0,
  "aim_x":0.8,
  "aim_y":-0.2,
  "shooting":true
}
```

`sequence` が最後に受理した値以下なら、古い入力として破棄する。

## サーバーからクライアント

- `welcome`: プレイヤーIDを割り当てる
- `rejected`: 定員超過などで参加を拒否する
- `snapshot`: 20Hzでプレイヤー、弾、試合状態を配信する

Rust側の正式な型定義は `protocol/src/lib.rs` を参照すること。

