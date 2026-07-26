# 通信プロトコル

第1段階では `ws://127.0.0.1:9001` にJSONテキストを送受信する。
クライアントは位置やHPを送らず、入力だけを送る。サーバーがすべてのゲーム状態を決定する。

## クライアントからサーバー

接続直後:

```json
{
  "type":"join",
  "name":"Player",
  "reconnect_token":"",
  "join_ticket":"..."
}
```

初回接続の`reconnect_token`は空文字列にする。`welcome`で受け取ったトークンを
保持し、WebSocket切断後のJoinで再送すると同じPlayer Entityへ復帰できる。
マッチング構成では、Matchmakerが返した`game_url`へ直接接続し、同時に返された
`join_ticket`を送る。単体ローカルサーバーでは`join_ticket`を省略できる。

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

ルームホストだけが送信できる操作:

```json
{"type":"add_cpu"}
{"type":"remove_cpu","player_id":3}
{"type":"start_match"}
```

ルーム設定:

```json
{
  "type":"update_room_settings",
  "settings":{
    "match_seconds":120,
    "kill_points":100,
    "death_penalty":25,
    "item_points":20,
    "item_spawn_interval":5,
    "max_items":3
  }
}
```

最初に入室した人間がホストになり、`start_match`でカウントダウンへ進む。
ホスト1人で`start_match`した場合は
サーバーがCPUを1体自動追加する。人間とCPUの合計は4人まで。

## サーバーからクライアント

- `welcome`: プレイヤーID、再接続トークン、再接続だったかを返す
- `rejected`: 定員超過などで参加を拒否する
- `snapshot`: 20Hzでプレイヤー、弾、得点アイテム、試合状態を配信する

`snapshot.map`には現在のマップの`id`、`revision`、グリッド寸法、
`tile_size`が含まれる。組み込みマップの地形本体は毎回送信せず、Godotは
ローカルのJSONと`id`・`revision`が一致することを確認する。

プレイヤーのスナップショットには、サーバーが最後に処理した
`last_input_sequence`が含まれる。Godotはこの番号までの予測入力を破棄し、
未処理の入力だけをサーバー確定位置へ再適用する。

弾のスナップショットには`position`と`velocity`が含まれる。Godotは20Hzの
受信間隔を`velocity`で外挿し、描画フレームごとに弾を滑らかに移動させる。

`items`には得点アイテムの`id`、`position`、`points`が含まれる。
アイテムは移動しないため、Godotは受信位置へそのまま表示する。取得判定と加点は
サーバーだけが行い、取得済みアイテムは次のスナップショットから消える。

スナップショットにはサーバー設定由来の `move_speed`、`dash_speed`、
`dash_duration`、`dash_cooldown` も含まれる。Godotはこの値で入力予測するため、
`server.json` で操作パラメーターを変更してもサーバーの確定計算と一致する。

試合フェーズ:

- `waiting`: 2人の参加待ち
- `countdown`: 試合開始カウントダウン
- `running`: 時間制ポイントマッチ
- `paused`: 切断者の再接続待ち
- `match_finished`: タイムアップ後または途中離脱後の試合結果

`players[].score`は符号付き整数で、死亡ペナルティにより負数になる場合がある。
`players[].is_cpu`でサーバー操作のCPUかを判別できる。
タイムアップ時は最高得点者のIDが`winner_id`へ入り、同点なら`null`になる。

`room`には`host_player_id`、`can_start`、`max_players`、現在のルーム設定が
含まれる。クライアントは`waiting`中、この情報をルーム画面へ表示する。

Rust側の正式な型定義は `backend/protocols/game/src/lib.rs` を参照すること。
