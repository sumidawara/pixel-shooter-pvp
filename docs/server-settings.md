# サーバー設定

サーバーは起動時にリポジトリ直下の `server.json` を読み込みます。
別のファイルを使う場合は `PIXEL_SHOOTER_CONFIG` にパスを指定します。
Docker Composeでも同じファイルを`/app/server.json`へ読み取り専用でマウントし、
両Game Serverが起動時に読み込みます。

```sh
PIXEL_SHOOTER_CONFIG=server.production.json \
./pixel-shooter-server
```

## セクション

- `network`: 待受アドレス、tick rate、スナップショット頻度、試験用の遅延と欠落率
- `control`: AdminServerだけが利用する内部API、公開URL、Ticket検証
- `match`: 試合時間、カウントダウン、得点、アイテム生成、再接続猶予
- `gameplay`: 移動、弾、反動、HP、リロード、無敵時間、ダッシュ、リスポーン
- `items`: アイテムを使ったときに起きることの数値
- `sandbox`: 練習場の手触り（アイテムの戻り、的の復活）
- `cpu`: CPUの強さ。段階ごとの数値を上書きする

`match`の主な項目:

- `match_seconds`: 試合時間
- `kill_points`: 相手を撃破したプレイヤーの加点
- `death_penalty`: 死亡したプレイヤーの減点
- `item_points`: 得点アイテム1個の加点
- `item_spawn_interval`: アイテムの生成間隔
- `max_items`: 同時に存在できるアイテム数

`gameplay`の`reload_seconds`は武器のリロードにかかる秒数です。設定変更後は
Game Serverを再起動すると反映されます。

## アイテムの効果

`items` にまとめてある。バランス調整でいちばんよく触る類なので、
組み立て直さずに試せるようにしてある。

- `berserk_seconds` / `berserk_bullet_speed_multiplier`: バーサクの効果時間と弾速の倍率
- `larokin_count` / `larokin_speed` / `larokin_radius` / `larokin_damage`: ラロキンポッポスの数・速さ・当たり判定・威力
- `larokin_telegraph_seconds`: 突撃を始めるまでの溜め。避ける余地を作るための間
- `ghost_thief_seconds`: ゴーストが飛んで戻るまでの時間（見せている時間だけで、
  奪取そのものは使用したtickで確定している）

`sandbox` は練習場だけに効く。`item_restock_seconds` は取られたアイテムが戻るまで、
`dummy_respawn_seconds` は的が起き上がるまでの時間。

## CPUの強さ

`cpu.levels` に段階1から順に並べる。書かなかった段階は組み込みの値を使うので、
調整したい段階だけを書けばよい。項目の意味は[ゲームルール](game-rules.md)にある。

```json
"cpu": {
  "levels": [
    {
      "sight_radius": 110.0,
      "reaction_ticks": 30,
      "aim_turn_degrees": 90.0,
      "aim_drift_degrees": 14.0,
      "fire_cone_degrees": 35.0,
      "strafe_bias": 0.0,
      "leads_target": false,
      "retreat_start_tiles": 0.0,
      "seeks_items": false
    }
  ]
}
```

配列の途中だけを書くことはできない（1番目が段階1、2番目が段階2）。
1つの段階の中で書かなかった項目は、段階3の値で埋まる。

極端な値は起動時に安全な範囲へ丸める。`aim_turn_degrees` を0にすると狙いが
永久に動かず、`strafe_bias` を1.0にすると真横だけを向いて近づかなくなるため、
どちらも下限・上限がある。

## ポートを探索するか、固定するか

`bind_address`のポートが他のプロセスに使われていることがあります。既定では
その先を順に試し、空いている番号で待ち受けます。`port_search_range`が
いくつ先まで試すかで、`0`にすると探索せず、指定した番号が使えなければ
起動しません。

```json
"network": { "bind_address": "127.0.0.1:9001", "port_search_range": 20 },
"control": { "bind_address": "127.0.0.1:9101", "port_search_range": 20 }
```

| 使い方 | 設定 | 理由 |
| --- | --- | --- |
| 手元で遊ぶ、配布版でCREATE ROOM | `20`（既定） | 遊ぶ人にとってポート番号はどうでもよい。埋まっているだけで部屋を作れないのは行き止まりになる |
| Docker Compose、公開サーバー | `0` | `ports:`や`public_url`で番号を外へ約束している。勝手にずれると誰も繋がらない |

実際に開いたアドレスは起動ログに出ます。番号が変わったときは
`9001 was busy; opened 127.0.0.1:9002 instead`のように理由も残ります。
`public_url`が古い番号を指したままなら警告します。

GodotのCREATE ROOMは、この設定に従ってサーバーが選んだ番号を受け取って
ルーム画面へ表示します。クライアント側では探索しないので、探索するかどうかは
`server.json`だけで決まります。開始する番号はゲーム内のSETTINGSで指定でき、
これは`--bind`として渡るため`bind_address`より優先されます。

これらはサーバー起動時のルーム初期値になる。Waiting中はルームホストが
GodotのCreate Room画面から安全な範囲内で上書きできる。

ルーム設定を決めるのはサーバーで、クライアントは受け取った設定を編集して返す。
クライアントは、届く前には何も送らず、画面に無い項目は受け取った値をそのまま返す。
この向きが崩れると、`server.json`へ書いた値が黙って効かなくなる
（`frontend/tests/room_settings_test.gd`が検査する）。

設定値が極端な場合はサーバー側で安全な範囲に補正します。ファイルがない、
またはJSONとして読めない場合は組み込みの初期値で起動します。

次の環境変数は、互換性とコンテナ運用のため `server.json` より優先されます。

- `PIXEL_SHOOTER_BIND_ADDR`
- `PIXEL_SHOOTER_PORT_SEARCH_RANGE`
- `PIXEL_SHOOTER_CONTROL_PORT_SEARCH_RANGE`
- `PIXEL_SHOOTER_LATENCY_MS`
- `PIXEL_SHOOTER_PACKET_LOSS_PERCENT`
- `PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS`
- `PIXEL_SHOOTER_CONTROL_BIND_ADDR`
- `PIXEL_SHOOTER_CONTROL_URL`
- `PIXEL_SHOOTER_SERVER_ID`
- `PIXEL_SHOOTER_PUBLIC_URL`
- `PIXEL_SHOOTER_ADMIN_URL`
- `PIXEL_SHOOTER_REQUIRE_JOIN_TICKET`
- `PIXEL_SHOOTER_JOIN_SECRET`

## Webデバッグ画面

デバッグ画面はGameServerから`admin-server`へ移動した。Docker Composeでは
`http://127.0.0.1:8081/debug/`で開き、対象GameServerを選んでSnapshotを確認し、
Pause、1 tick Step、Resumeを操作できる。GameServerの9101番Control APIは
内部ネットワーク用であり、インターネットへ直接公開しないこと。

`control.require_join_ticket`を有効にしたGameServerへ新規参加するには、
同じ`control.join_secret`を持つMatchmakerが発行したTicketが必要になる。
本番では既定の秘密鍵を必ず十分長いランダム値へ変更する。
