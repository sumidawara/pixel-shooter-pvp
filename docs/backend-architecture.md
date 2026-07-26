# バックエンド構成

## GameCoreとServerRuntime

ゲーム計算は、実時間ループから独立したBevyの`GameTick` Scheduleに登録する。
`GameTick`を1回実行すると、ゲーム世界がちょうど1tick進む。

```text
ScheduleRunnerPlugin
└── FixedUpdate（ServerRuntime）
    ├── WebSocketイベントをPlayerへ反映
    ├── GameTickを1回実行
    │   ├── 試合フェーズ更新
    │   ├── CPU入力決定
    │   ├── プレイヤー移動
    │   ├── 射撃
    │   ├── 弾移動・当たり判定
    │   ├── アイテム更新
    │   └── リスポーン
    └── Snapshot送信
```

- `game_core.rs`: `GameTick`とゲームSystemの実行順を定義する
- `server_runtime.rs`: 通信処理と`GameTick`、Snapshot送信を接続する
- `main.rs`: 設定とResourceを組み立て、両Pluginを登録する

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

次の段階ではServerRuntimeへ`Realtime`と`Paused`の実行モードを追加し、
`Paused`中の`step`要求だけが`GameTick`を進めるようにする。
