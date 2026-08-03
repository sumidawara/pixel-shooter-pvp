# サーバー群の起動と配布

## Docker Compose

開発用の固定2台GameServerプールは、incrementalビルドを使う共有イメージで起動する。

```sh
PIXEL_SHOOTER_JOIN_SECRET='replace-with-a-long-random-secret' make dev
```

本番相当のreleaseイメージを使う場合は、専用Composeファイルを指定する。

```sh
PIXEL_SHOOTER_JOIN_SECRET='replace-with-a-long-random-secret' \
  docker compose -f docker-compose.release.yml up --detach --build
```

| ポート | サービス |
| --- | --- |
| `8080` | 公開Matchmaker API |
| `8081` | AdminServerデバッグ画面 |
| `9001` | GameServer 1 WebSocket |
| `9002` | GameServer 2 WebSocket |

GameServerのControl API（コンテナ内9101番）はホストへ公開しない。
固定台数を変える場合は対象のComposeファイルへGameServerサービスを追加し、
一意な`PIXEL_SHOOTER_SERVER_ID`、外部`PIXEL_SHOOTER_PUBLIC_URL`、
内部`PIXEL_SHOOTER_CONTROL_URL`を設定する。

開発用の`docker-compose.yml`は`backend/maps`を読み取り専用でマウントしている。
マップ変更後は`make reload-maps`でGame Serverだけを再起動する。release構成では
マップをバイナリへ埋め込むため、変更時にreleaseイメージを再ビルドする。

Godotの`JOIN ROOM`には`http://127.0.0.1:8080`を入力する。クライアントは
MatchmakerへHTTP POSTし、返された`game_url`へJoin Ticket付きWebSocketで直接接続する。
従来どおり`ws://127.0.0.1:9001`を入力すれば、Ticket不要設定の単体サーバーへ直結できる。

enosawaのTailnet向け環境では、次のHTTPS/WSSエンドポイントを使用する。

| URL | サービス |
| --- | --- |
| `https://matchmaker.pvp.es.sumidawara.uk` | Matchmaker API |
| `https://admin.pvp.es.sumidawara.uk` | AdminServerデバッグ画面 |
| `wss://game1.pvp.es.sumidawara.uk` | GameServer 1 |
| `wss://game2.pvp.es.sumidawara.uk` | GameServer 2 |

Godotの`JOIN ROOM`には`https://matchmaker.pvp.es.sumidawara.uk`を入力する。
AdminServerは操作APIを持つため、`admin.pvp.es.sumidawara.uk`はTailnet内だけで公開する。

## 個別プロセス

```sh
cargo run -p pixel-shooter-admin-server
cargo run -p pixel-shooter-matchmaker
cargo run -p pixel-shooter-server
```

GameServerをAdminServerへ登録する場合は、少なくとも
`PIXEL_SHOOTER_ADMIN_URL`、`PIXEL_SHOOTER_CONTROL_URL`、
`PIXEL_SHOOTER_PUBLIC_URL`を設定する。

## 統合試験

3サービスを起動した後に次を実行する。

```sh
node scripts/control_plane_test.mjs
```

この試験は同一ルームへの割当、有効Ticketの受理、TicketなしJoinの拒否、
Pause中のtick停止、Stepによる厳密な1 tick進行、Resumeを検証する。
