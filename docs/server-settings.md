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

`match`の主な項目:

- `match_seconds`: 試合時間
- `kill_points`: 相手を撃破したプレイヤーの加点
- `death_penalty`: 死亡したプレイヤーの減点
- `item_points`: 得点アイテム1個の加点
- `item_spawn_interval`: アイテムの生成間隔
- `max_items`: 同時に存在できるアイテム数

`gameplay`の`reload_seconds`は武器のリロードにかかる秒数です。設定変更後は
Game Serverを再起動すると反映されます。

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
