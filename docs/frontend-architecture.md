# Godotフロントエンド構成

`Main`はメニューとゲーム画面の切り替えだけを担当します。通信、ゲーム表示、
UIを別シーンへ分け、Godotエディターのシーンツリーから画面構造を確認できます。

```text
Main
├── HostServerController
├── MenuScreen
│   ├── TitlePage
│   ├── PlayPage
│   ├── JoinPage
│   ├── CreatePage
│   └── SettingsPage
└── GameScreen
    ├── World
    │   ├── Arena
    │   ├── ItemLayer
    │   ├── BulletLayer
    │   ├── PlayerLayer
    │   └── EffectLayer
    ├── Audio
    └── HUD
```

## データの流れ

1. `MenuScreen`がルーム作成または接続先を`Main`へ通知する。
2. Create Roomでは`HostServerController`が同梱Rustサーバーを起動する。
3. Autoloadの`NetworkClient`がWebSocketを接続し、サーバーへ入力を送る。
4. Waiting中はSnapshotの`room`と`players`をCreatePageへ表示する。
5. 試合開始後は`GameScreen`がPlayerView、BulletView、ItemView、HUDへ振り分ける。
6. 自分の位置だけは`GameScreen`が入力予測し、サーバー確定位置で補正する。

## 主なシーン

- `src/ui/menu/menu_screen.tscn`: タイトル、Play、ルーム、設定画面
- `src/game_modes/match/game_screen.tscn`: 対戦画面全体
- `src/actors/player/player_view.tscn`: プレイヤー1体の表示
- `src/combat/projectiles/bullet_view.tscn`: 弾1発の表示
- `src/combat/items/item_view.tscn`: 得点アイテム1個の表示
- `src/ui/hud/hud.tscn`: 対戦HUDとオーバーレイ
- `src/ui/hud/player_status.tscn`: 1人分のHP、弾数、ダッシュ表示

色やフォントなどの共通UI設定は `src/shared/themes/pixel_theme.tres` にまとめています。
