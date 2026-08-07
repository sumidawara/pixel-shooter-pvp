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

## ルーム割当と参加可否

AdminServerは、まず参加枠の残っている既存ルームへ合流させ、無ければ空きGameServerへ
新しいルームを割り当てる。合流先の判定にはGameServerがheartbeatで報告する
`accepting_players`（`MatchPhase::Waiting` かつ定員未満）を使う。

参加可否を決めるのはGameServerの試合フェーズと人数であり、AdminServerはそれを知らない。
報告しない場合、AdminServerは走行中のルームへ案内してしまい、プレイヤーはGameServerに
拒否されて行き止まりになる（空きGameServerが隣にあっても）。

Join Ticketを発行した時点で1席を確保し、`RESERVATION_TTL`（Ticketの有効期限と同じ60秒）
を過ぎても接続が来なければ席を返す。返さないと、拒否された割当や離脱したぶんだけ
席が減り続ける。

heartbeat間隔ぶんのすれ違いは残るため、GameServerは満室・試合開始済みの拒否を
`retryable: true` で返し、クライアントは別のルームを取り直す。

## ゲーム規則の単一実装

被弾の適用（シールド消費、無敵時間、死亡判定、スコア増減）は
`backend/game-core/src/game/damage.rs` に集約する。弾もラロキンポッポスも
この関数だけを呼ぶ。ダメージ源ごとに条件が分かれていると、
「弾では無敵時間が付くのにラロキンでは付かない」といった差異が静かに生まれる。

移動・ダッシュ・壁判定はクライアント予測のため、GDScript側にも同じ規則が存在する。
`backend/game-core/tests/movement_prediction_golden.rs` が「入力列 → 位置列」を
fixtureとして固定し、`frontend/tests/movement_prediction_golden_test.gd` が
同じ入力を予測側へ流して一致を検証する。片側だけを変えると必ずテストが落ちる。

## Join Ticket

Matchmakerは`room_id`、プレイヤー名、有効期限をJSON化し、共有秘密鍵による
HMAC-SHA256署名を付ける。GameServerは新規Join時に署名、有効期限、割当済み
`room_id`を検証する。再接続は既存のランダムな`reconnect_token`で同じEntityへ戻す。

ローカルの単体GameServerは互換性のためTicket不要が初期値だが、
Docker Composeでは`PIXEL_SHOOTER_REQUIRE_JOIN_TICKET=true`を設定する。
