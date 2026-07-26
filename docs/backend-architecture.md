# バックエンド構成

## サービス境界

```text
Godot
  ├─ POST /v1/matchmake ──> Matchmaker
  │                            └─ 割当要求 ──> AdminServer
  │                                           └─ 空き1台を確保 ──> GameServer pool
  └─ Join Ticket付きWebSocket ────────────────────────────────> GameServer

AdminServer
  ├─ GameServerの登録・Heartbeat・割当
  ├─ Pause / Step / Resumeの中継
  └─ Svelteデバッグ画面
```

- `backend/game-core`: 通信や実時間を知らないゲームルールのRustライブラリ
- `backend/game-server`: 1プロセス＝1ルームのBevy権威サーバー
- `backend/matchmaker`: 割当を要求し、署名済みJoin Ticketを発行する公開API
- `backend/admin-server`: 固定GameServerプールと内部Control APIを管理する
- `backend/protocols/game`: GodotとGameServer間のゲーム通信型
- `backend/protocols/admin`: サーバー間の管理通信型とJoin Ticket型

## GameCoreとServerRuntime

ゲーム計算は、実時間ループから独立したBevyの`GameTick` Scheduleに登録する。
`GameTick`を1回実行すると、ゲーム世界がちょうど1tick進む。

```text
ScheduleRunnerPlugin
└── FixedUpdate（ServerRuntime）
    ├── ControlコマンドとWebSocketイベントを処理
    ├── Realtime、またはStep要求時だけGameTickを1回実行
    │   ├── 試合フェーズ更新
    │   ├── CPU入力決定
    │   ├── プレイヤー移動
    │   ├── 射撃
    │   ├── 弾移動・当たり判定
    │   ├── アイテム更新
    │   └── リスポーン
    ├── Snapshot送信
    └── Control状態とHeartbeatを更新
```

- `backend/game-core/src/schedule.rs`: `GameTick`とゲームSystemの実行順
- `backend/game-server/src/server_runtime.rs`: 通信・管理と`GameTick`を接続
- `backend/game-server/src/control.rs`: 内部Control APIとデバッグ実行モード

通信を`GameTick`の外側へ置くことで、今後ゲーム世界を一時停止しても、
接続維持、管理コマンド、Heartbeatを処理し続けられる。

## 依存方向

```text
ServerRuntime
├── Network
└── GameCore
    ├── Game Systems
    ├── Model
    └── Arena
```

GameCoreは、WebSocketの待受ポートやTokioの実行方法を知らない。
ゲームSystemは実時間の`Time`ではなく、GameCoreが持つ固定`GameClock`を参照する。
テストや将来のデバッグ制御は、実時間ランナーを起動せず
`advance_one_tick`を必要な回数だけ呼び出せる。

`Paused`中も外側の`FixedUpdate`は動くため、WebSocket、管理API、Heartbeatは停止しない。
`StepRequest { ticks: 1 }`を受けると、次の外側ループで`GameTick`だけを1回進める。

## Join Ticket

Matchmakerは`room_id`、プレイヤー名、有効期限をJSON化し、共有秘密鍵による
HMAC-SHA256署名を付ける。GameServerは新規Join時に署名、有効期限、割当済み
`room_id`を検証する。再接続は既存のランダムな`reconnect_token`で同じEntityへ戻す。

ローカルの単体GameServerは互換性のためTicket不要が初期値だが、
Docker Composeでは`PIXEL_SHOOTER_REQUIRE_JOIN_TICKET=true`を設定する。
