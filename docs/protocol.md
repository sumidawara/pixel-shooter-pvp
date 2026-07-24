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
  "shooting":true,
  "reload_pressed":false,
  "dash_pressed":true
}
```

`sequence` が最後に受理した値以下なら、古い入力として破棄する。
`reload_pressed`と`dash_pressed`はキーを押した瞬間だけ`true`にする。

## サーバーからクライアント

- `welcome`: プレイヤーIDを割り当てる
- `rejected`: 定員超過などで参加を拒否する
- `snapshot`: 20Hzでプレイヤー、弾、試合状態を配信する

プレイヤーのスナップショットには、サーバーが最後に処理した
`last_input_sequence`が含まれる。Godotはこの番号までの予測入力を破棄し、
未処理の入力だけをサーバー確定位置へ再適用する。

弾のスナップショットには`position`と`velocity`が含まれる。Godotは20Hzの
受信間隔を`velocity`で外挿し、描画フレームごとに弾を滑らかに移動させる。

Rust側の正式な型定義は `protocol/src/lib.rs` を参照すること。
