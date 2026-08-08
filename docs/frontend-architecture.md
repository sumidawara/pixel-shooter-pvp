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
    ├── FollowCamera
    ├── Audio
    └── HUD
```

`FollowCamera`は`World`の外に置いています。画面揺れは`World`ごと動かして
表現しているため、中に入れるとカメラも一緒に揺れて打ち消し合います。

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

## 画面の割り当て

画面は640×400で、上下の帯がHUD、あいだがマップです。

| 範囲 | 中身 |
| --- | --- |
| y 0〜45 | 持ち物、残り時間、接続状態、ESCの案内 |
| y 45〜359 | マップ（`FollowCamera`が映す範囲） |
| y 359〜400 | プレイヤー4人分のHP・弾数・ダッシュ |

HUDをマップの上へ重ねないのは、遮蔽の裏や足元が読めなくなるためです。
持ち物や残り時間はずっと出ているので、重ねると覗き込む場所を奪い続けます。

帯の位置は `src/ui/hud/hud.gd` の `WORLD_VIEW_TOP` / `WORLD_VIEW_BOTTOM` が持ち、
カメラの寄せ止め範囲もこの値から決めます。2箇所に書くと、片方だけ動かしたときに
マップの端がHUDの下へ潜ります。`frontend/tests/game_view_test.gd` が、
HUDが帯からはみ出さないことと、隅でマップの角が帯の角に重なることを検査します。

カメラは自機を画面の中心に置いて追い、`FOLLOW_ZOOM`（現在1.5）で寄ります。
寄り具合を変えるのはこの定数だけです。狙いは画面座標ではなくワールド座標で
取ります。カメラが動くと両者はずれるため、画面座標のままだと狙った所と
撃つ向きが食い違います。

色やフォントなどの共通UI設定は `src/shared/themes/pixel_theme.tres` にまとめています。
