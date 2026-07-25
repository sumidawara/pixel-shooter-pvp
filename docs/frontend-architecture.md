# Godotフロントエンド構成

`Main`はメニューとゲーム画面の切り替えだけを担当します。通信、ゲーム表示、
UIを別シーンへ分け、Godotエディターのシーンツリーから画面構造を確認できます。

```text
Main
├── MenuScreen
│   ├── Background
│   └── ConnectionPanel
└── GameScreen
    ├── World
    │   ├── Arena
    │   ├── BulletLayer
    │   ├── PlayerLayer
    │   └── EffectLayer
    ├── Audio
    └── HUD
```

## データの流れ

1. `MenuScreen`が接続先とプレイヤー名を`Main`へ通知する。
2. Autoloadの`NetworkClient`がWebSocketを接続し、サーバーへ入力を送る。
3. `NetworkClient`が受信したSnapshotをSignalで`GameScreen`へ渡す。
4. `GameScreen`がPlayerView、BulletView、HUDへ表示データを振り分ける。
5. 自分の位置だけは`GameScreen`が入力予測し、サーバー確定位置で補正する。

## 主なシーン

- `scenes/ui/menu_screen.tscn`: 接続画面
- `scenes/game/game_screen.tscn`: 対戦画面全体
- `scenes/game/player_view.tscn`: プレイヤー1体の表示
- `scenes/game/bullet_view.tscn`: 弾1発の表示
- `scenes/ui/hud.tscn`: 対戦HUDとオーバーレイ
- `scenes/ui/player_status.tscn`: 1人分のHP、弾数、ダッシュ表示

色やフォントなどの共通UI設定は `themes/pixel_theme.tres` にまとめています。
