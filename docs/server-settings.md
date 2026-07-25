# サーバー設定

サーバーは起動時にリポジトリ直下の `server.json` を読み込みます。
別のファイルを使う場合は `PIXEL_SHOOTER_CONFIG` にパスを指定します。

```sh
PIXEL_SHOOTER_CONFIG=server.production.json \
./pixel-shooter-server
```

## セクション

- `network`: 待受アドレス、tick rate、スナップショット頻度、試験用の遅延と欠落率
- `match`: 試合時間、カウントダウン、得点、アイテム生成、再接続猶予
- `gameplay`: 移動、弾、反動、HP、リロード、無敵時間、ダッシュ、リスポーン

`match`の主な項目:

- `match_seconds`: 試合時間
- `kill_points`: 相手を撃破したプレイヤーの加点
- `death_penalty`: 死亡したプレイヤーの減点
- `item_points`: 得点アイテム1個の加点
- `item_spawn_interval`: アイテムの生成間隔
- `max_items`: 同時に存在できるアイテム数

これらはサーバー起動時のルーム初期値になる。Waiting中はルームホストが
GodotのCreate Room画面から安全な範囲内で上書きできる。

設定値が極端な場合はサーバー側で安全な範囲に補正します。ファイルがない、
またはJSONとして読めない場合は組み込みの初期値で起動します。

次の環境変数は、互換性とコンテナ運用のため `server.json` より優先されます。

- `PIXEL_SHOOTER_BIND_ADDR`
- `PIXEL_SHOOTER_LATENCY_MS`
- `PIXEL_SHOOTER_PACKET_LOSS_PERCENT`
- `PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS`
