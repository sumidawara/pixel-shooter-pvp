# Pixel Shooter PvP

GodotフロントエンドとRust + Bevy権威サーバーで動く、最大4人の見下ろし型シューティングゲーム。
120秒で撃破とアイテム取得によるポイントを競い、死亡時には得点ペナルティが入る。

## 必要環境

- Rust（版は`rust-toolchain.toml`で固定。rustupが自動で切り替える）
- Godot 4.7以降
- DockerとDocker Compose
- Node.jsとnpm（Adminデバッグ画面と統合試験）
- Make

## 開発クイックスタート

利用できるコマンドは`make help`で確認できる。

```sh
make help
make doctor
make setup
```

コミット前の検査は`make verify`にまとめている。Rustの整形・Clippy・テストに加え、
Adminデバッグ画面の型検査と、Godotクライアントのテストを実行する。

```sh
make verify
```

`make test-frontend`はGameServerのバイナリが無いと2本を`SKIP`として明示する。
未実行を残したくない場面では`make test-frontend-full`を使う。

サーバー側のゲーム規則や通信フォーマットを変更した場合は、クライアントとの
契約テストの期待値を再生成し、Godot側を追従させる。

```sh
make update-goldens
make test-frontend
```

サービス間の結合はCompose環境と単体GameServerに対して確認する。

```sh
make build-game-server
make integration-server
make dev
make integration
```

ブランチ運用、変更の種類ごとの手順、CIが見る範囲と見られない範囲は
[`docs/development-flow.md`](docs/development-flow.md)にまとめている。

通常の開発では、Admin Server、Matchmaker、Game Server 2台をまとめて起動する。

```sh
make dev
```

`make dev`は全Rustバイナリをincrementalな開発プロファイルで1つの共有イメージへ
ビルドし、バックエンド全体を起動する。Game Serverの登録完了後に接続先を表示する。
ビルド済みイメージをそのまま起動する場合は`make up`、バックエンド変更を反映する
場合は`make rebuild`を使う。本番相当のreleaseビルドは`make rebuild-release`で
明示的に作成する。

```sh
make ps
make logs
make logs SERVICE=matchmaker
make integration
make reload-maps
make down
```

開発用Composeは`backend/maps`をGame Serverへ直接マウントする。マップJSONの
変更後はイメージを再ビルドせず、`make reload-maps`でGame Server 2台だけを再起動
すれば反映できる。

GodotがPATHにある場合は、次のコマンドでプロジェクトを開ける。

```sh
make godot
```

macOSでGodotをPATHへ追加していない場合:

```sh
make godot GODOT_BIN=/Applications/Godot.app/Contents/MacOS/Godot
```

Macとバックエンドが別マシンの場合は、Mac側のリポジトリでSSHトンネルを起動する。

```sh
make tunnel SSH_HOST=backend-host
```

Godotの`JOIN ROOM`へ`http://127.0.0.1:8080`を入力すると、Matchmakerが
Game Serverを割り当て、Join Ticketを発行し、そのGame Serverへ直接接続する。
Admin Serverのデバッグ画面は`http://127.0.0.1:8081/debug/`で確認できる。
対象GameServerの選択、マップ・Entity・Snapshotの表示に加え、
Pause、1 tick Step、Resume、モデル入力シナリオの注入を操作できる。
入力列の共通JSON形式とAPIは
[`docs/input-scenarios.md`](docs/input-scenarios.md)、構成全体は
[`docs/deployment.md`](docs/deployment.md)を参照。

Godotクライアントの接続先は
[`frontend/src/networking/network_config.gd`](frontend/src/networking/network_config.gd)へ集約している。
一時的に別の接続先を使う場合は、起動時に`PIXEL_SHOOTER_SERVER_URL`環境変数でも
上書きできる。

Adminデバッグ画面を変更中は、Compose環境を起動したままViteのHMRを利用できる。

```sh
make web-install
make web-dev
```

埋め込み用の配布アセットを更新する場合:

```sh
make web-build
```

## Game Server単体での開発

ゲームルールやWebSocket通信だけを素早く確認する場合は、Dockerを介さず直接起動できる。
Compose環境が起動中なら9001番ポートが競合するため、先に`make down`で停止する。

```sh
make run-game-server
```

Godotの`JOIN ROOM`へ`ws://127.0.0.1:9001`を入力する。最初に入室したプレイヤーが
ホストになり、CPU追加、ルール設定、試合開始を操作できる。2人で確認する場合は、
エディター設定の「複数インスタンスを実行」を有効にするか、CPUを追加する。
ホスト1人のままSTART GAMEを押した場合は、対戦相手のCPUが1体自動追加される。

デスクトップ版の`CREATE ROOM`はRustサーバーを子プロセスとして起動する。
Godotエディターでは`target/debug/pixel-shooter-server`を自動検出するため、先に
`make build-game-server`を実行しておく。

操作:

- WASD: 移動
- マウス: 照準
- 左クリック: 射撃
- R: リロード
- Shift: 移動入力方向へ通常ダッシュ
- Space: 所持しているスロットアイテムを使用
- Esc: 接続を終了してメニューへ戻る

クライアントは自分の移動を入力予測し、他プレイヤーは受信した位置の間を補間する。
サーバーの確定位置と差が出た場合は、未処理入力を再適用して滑らかに補正する。

## サーバー設定

ゲームルールとネットワーク設定は [`server.json`](server.json) で変更できる。
設定ファイルの全項目と環境変数による上書きは
[`docs/server-settings.md`](docs/server-settings.md) を参照。

GameServerの`control`設定はAdminServerへの登録、内部API、Join Ticket検証に使う。
Control APIは外部へ直接公開せず、AdminServerを介して操作する。

## 遅延・パケット欠落試験

サーバーは環境変数で、スナップショットの片道遅延と欠落率を模擬できる。

```sh
PIXEL_SHOOTER_LATENCY_MS=120 \
PIXEL_SHOOTER_PACKET_LOSS_PERCENT=20 \
make run-game-server
```

9001番ポートを別のサーバーが使用中なら、試験用ポートも変更できる。

```sh
PIXEL_SHOOTER_BIND_ADDR=127.0.0.1:9002 \
PIXEL_SHOOTER_LATENCY_MS=120 \
PIXEL_SHOOTER_PACKET_LOSS_PERCENT=20 \
make run-game-server
```

別のターミナルで自動試験クライアントを実行する。

```sh
node scripts/network_test.mjs
```

切断・再接続・一時停止の試験:

```sh
node scripts/reconnect_test.mjs
```

途中離脱の短縮試験では、再接続猶予だけを1秒にできる。

```sh
PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS=1 make run-game-server
node scripts/forfeit_test.mjs
node scripts/cpu_orphan_test.mjs
```

ポートを変更した場合:

```sh
PIXEL_SHOOTER_SERVER_URL=ws://127.0.0.1:9002 node scripts/network_test.mjs
```

この模擬はクライアントからの入力と、サーバーからのスナップショットに適用される。
`PIXEL_SHOOTER_LATENCY_MS`は片道の遅延値である。参加処理を検証可能に保つため、
`join`、`welcome`、`rejected`には適用しない。

## ビルドと配布

Godotの「エディター → エクスポートテンプレートの管理」から4.7用テンプレートを
インストールした後、次のコマンドで配布物を作成する。

```sh
make release
```

`RELEASE_TARGET`で対象を選べる。既定は`linux`。GodotがPATHにない場合は`GODOT_BIN`で
実行ファイルを指定する。出力はGit管理外の`dist/`に作られる。

| 対象 | 状態 |
| --- | --- |
| `linux` | 動作確認済み |
| `windows` | クライアントはLinuxからも書き出せるが、GameServerがホスト向けのまま同梱される。Windows上でビルドするか、クロスコンパイルの対応が必要 |
| `macos` | プロジェクト設定が足りず現状書き出せない（arm64を含むにはETC2 ASTCの取り込みが必要）。署名も macOS 実機が要る |
| `pck` | ゲームデータだけ。エクスポートテンプレート不要 |
| `server` | GameServerだけ。Godot不要 |

Linux版の生成物は次のようになる。クライアントは同じ階層の`server/`から
GameServerを探すので、この配置のまま配布する。

```text
dist/
├── PixelShooterPvP.x86_64   クライアント本体
├── PixelShooterPvP.pck      ゲームデータ
└── server/
    ├── pixel-shooter-server GameServer（CREATE ROOMが起動する）
    └── server.json          サーバー設定
```

サーバーだけを配る場合は`RELEASE_TARGET=server`を使う。こちらはGodotを必要としない。

エクスポートテンプレートなしでクライアントのパックだけを検証する場合:

```sh
make release RELEASE_TARGET=pck
```

画像はAseprite原本(`frontend/assets/aseprite/`)だけを管理し、書き出し済みPNGは
持たない。`frontend/addons/aseprite_importer`が`.aseprite`をそのままテクスチャとして
読み込むため、原本を保存すればGodotが取り込む。出典と効果音の再生成方法は
[`docs/assets.md`](docs/assets.md) に記載している。

## 構成

- `frontend/`: Godotクライアント
  - `src/app/`: メニューとゲーム画面を配置するルート
  - `src/actors/`: プレイヤーなどのActor
  - `src/combat/`: 弾やアイテムなどの戦闘要素
  - `src/game_modes/`: 対戦進行とエフェクト
  - `src/maps/`: Arenaなどのマップ
  - `src/networking/`: 接続設定とデスクトップ版Rustサーバーの制御
  - `src/autoload/`: AutoloadのWebSocketクライアント
  - `src/ui/`: 接続メニュー、HUD、プレイヤー状態表示
  - `src/shared/`: 共通の音声、フォント、テーマ
- `backend/game-core/`: 実時間と通信から独立した`GameTick`とゲームルール
- `backend/game-server/`: 1プロセス＝1ルームのWebSocket権威サーバー
- `backend/matchmaker/`: GameServer割当要求とJoin Ticket発行
- `backend/admin-server/`: サーバープール管理、Control中継、デバッグ画面
- `backend/protocols/game/`: GodotとGameServer間のゲーム通信型
- `backend/protocols/admin/`: サーバー間の管理通信型とTicket署名
- `docs/`: プロトコル、ゲームルール、バックエンド構成、開発フロー
- `docker-compose.yml`: 固定数GameServerプールの開発構成
- `Makefile`: 開発、検査、Compose操作、リリースの共通コマンド
- `server.json`: サーバーの運用・ゲーム設定
- `scripts/build_release.sh`: サーバーとクライアントの配布ビルド

Godot側のシーン構成とデータの流れは
[`docs/frontend-architecture.md`](docs/frontend-architecture.md) を参照。
バックエンドのtick実行フローは
[`docs/backend-architecture.md`](docs/backend-architecture.md) を参照。
