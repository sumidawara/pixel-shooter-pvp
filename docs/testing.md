# テスト

**どんなテストがあり、それぞれ何を守っているか**の一覧。
いつ回すか・CIの必須チェック・mainの保護は
[`development-flow.md`](development-flow.md) を参照。

このプロジェクトのテストは4つの層に分かれている。層ごとに「何が壊れたら落ちるか」が
違うので、テストを足すときはまずどの層の話かを決める。

| 層 | 数 | 守っているもの | 実行 |
| --- | --- | --- | --- |
| Rust 単体 | 128 | ゲーム計算、設定の解釈、サーバーの部品 | `make test` |
| 契約（ゴールデン） | 3組 | RustとGodotが同じ前提で動いていること | 上の2つに含まれる |
| Godot 画面 | 22本 | 画面の作りと、失敗したときの逃げ道 | `make test-frontend` |
| 統合（実サーバー） | 8本＋Compose1本 | 実際に繋いだときの通しの動き | `make integration-server` / `make integration` |

`make verify` が下3つ以外をまとめて実行する（整形・Clippy・Rust・Web型検査・Godot）。
コミット前はこれを通す。統合試験だけは別で、サーバーの実行ファイルが要る。

## 契約テスト — この設計の要

同じ規則がRustとGodotの両方に書かれている箇所が3つある。型では守れないので、
**Rustを正としてJSONへ書き出し、Godot側がそれを読んで突き合わせる**。
どちらか一方だけを変えると必ず落ちる。

| ゴールデン | 何を縛るか | 生成元 | 突き合わせ |
| --- | --- | --- | --- |
| `movement_prediction_golden.json` | クライアント予測がサーバーの権威計算と一致すること | `game-core/tests/movement_prediction_golden.rs` | `frontend/tests/movement_prediction_golden_test.gd` |
| `wire_messages_golden.json` | 通信メッセージのフィールド名 | `protocols/game/tests/wire_golden.rs` | `frontend/tests/snapshot_contract_test.gd` |
| `shared_limits_golden.json` | 両側が守る範囲（ルーム設定の許容値、マップの上限、CPUの段階数、色の数） | `protocols/game/tests/limits_golden.rs` | `frontend/tests/shared_limits_test.gd` |

サーバー側の規則を変えたら期待値を作り直す。

```bash
make update-goldens
```

作り直したあとは必ず `make test-frontend` を通し、Godot側を追従させること。
**ゴールデンだけ更新して通ったつもりになるのが一番危ない。** 更新は「Godot側も直す」
という宣言であって、ずれを消す作業ではない。

`frontend/src/networking/snapshot_contract.gd` は、クライアントが実際に読むキーの
一覧である。GDScriptは `dictionary.get(key, default)` で読むため、サーバーが名前を
変えても例外は出ず、黙って既定値に落ちる。読むキーを増やしたらここにも足す。

## Rust 単体テスト

`cargo test --workspace` で全部走る。crate ごとの分担は次のとおり。

### game-core（ゲーム計算）

I/Oを持たず、同じ入力から必ず同じ結果になる。乱数を使わないので、
テストはすべて決定的に書ける。

- `game/cpu.rs`（15件）— 視界・反応・狙い・探索。**段階が本当に段階になっているか**を、
  生成した地形で8試合ずつ総当たりで戦わせて測る。勝った試合数ではなく合計得点で
  見るのは、1試合が湧き位置の運でひっくり返るため
- `game/items.rs`（7件）— アイテムの効果と対象選択。設定した数値が実際に使われることも見る
- `game/sandbox.rs`（8件）— 練習場の4つの決め事（的、全種類のアイテム、終わらない試合、得点を動かさない）
- `game/damage.rs`（5件）— 弾とラロキンポッポスで被弾結果が同じであること
- `arena/generator.rs`（9件）— 自動生成マップが遊べる形になっていること。**種400通りで毎回検査**
- `player_color.rs`（6件）— 色が重ならず、抜けた色が次の人へ回ること
- `cpu_skill.rs`（6件）— 段階ごとの能力が単調であること、極端な設定が丸められること
- `arena.rs` / `navigation.rs` / `schedule.rs` / `match_log.rs` / `input.rs` — マップ検証、経路探索、1tick進行、出来事の記録

### game-server（通信と設定）

- `config.rs`（5件）— `server.json` の探索順、極端な値の補正、**配る設定ファイルに項目が載っていること**
- `bind.rs` — 空きポートの探索と、固定指定のときに動かないこと
- `control.rs` / `maps.rs` / `network/snapshot.rs` — 1tickデバッグ、マップカタログ、通信の間引き
- `tests/toolchain_pin.rs` — ツールチェーンの固定が一貫していること

### matchmaker / admin-server / protocols

- `matchmaker`（11件）— 入場券に加えて、公開用のルーム一覧。**制御面のURLが混ざらないこと**を含む
- `admin-server/routes/registry.rs`（7件）— 空きサーバーの選び方と失効の扱い
- `protocols/admin`（2件）— 入場券の署名と改竄検知
- `protocols/game`（4件）— メッセージの往復

## Godot 画面テスト

`godot --headless` で1本ずつ実行する。合否は次の3つで決める
（`scripts/run_frontend_tests.sh`）。

- `quit()` の終了コード
- タイムアウト（既定120秒、`FRONTEND_TEST_TIMEOUT`で変更可）。実行中のエラーは
  プロセスを止めるので、戻ってこないこと自体が失敗になる
- 出力に `SCRIPT ERROR` が出ていないこと。構文エラーは終了コードに出ないことがある

**出力の `ERROR:` を丸ごと拾ってはいけない。** Godotは終了時に無関係な
`ERROR: N resources still in use at exit` を出すことがあり、全部通っているのに
落ちる判定になる。`SCRIPT ERROR` に限っているのはそのため。

| テスト | 守っているもの |
| --- | --- |
| `movement_prediction_golden_test` | 予測がサーバーと一致 |
| `snapshot_contract_test` | 読むキーが通信に存在する |
| `shared_limits_test` | 入力欄・マップ検証・段階数がサーバーと一致 |
| `room_settings_test` | ルーム設定がサーバーの持ち物として扱われている |
| `game_view_test` | HUDがマップに重ならない、カメラが自機を追う、狙いがワールド座標 |
| `player_facing_test` | 絵が進む向きを向き、止まっても正面へ戻らない |
| `lobby_layout_test` | 設定が属する対象の行にあり、色が並び順で変わらない |
| `room_list_test` | 満室が押せない、モーダルが一覧を消さない、断られたら一覧へ戻る |
| `public_address_test` | 一覧へ名乗るアドレスがループバックにならない |
| `sandbox_ui_test` | 練習場の設定が往復し、画面から分かる |
| `host_server_test` | CREATE ROOM の失敗経路と、同梱サーバーの生存監視 |
| `room_flow_test` / `join_room_flow_test` | ロビーから試合開始までの通し |
| `connection_cancel_test` / `rejection_retry_test` | 接続の中止と、拒否されたときの取り直し |
| `crt_ui_test` | CRT表現の切り替えとプレイヤー識別色 |
| `button_press_mode_test` | すべてのボタンが押した瞬間に反応する |
| `exit_confirm_modal_test` / `result_podium_test` / `ghost_thief_view_test` | 個別の画面部品 |
| `sprite_assets_test` / `aseprite_document_test` | 絵が読み込め、実際に見える状態か |

`host_server_test` / `join_room_flow_test` / `room_flow_test` は実際に動いている
サーバーを必要とする。`make test-frontend` が自動で1台（9019番）起動する。
実行ファイルが無いときは、この3本だけSKIPして残りを走らせる。

## 統合試験（実サーバー相手）

Node の WebSocket クライアントで、実際に起動したサーバーへ繋いで確かめる。

```bash
make build-game-server
make integration-server
```

**1本ごとにサーバーを作り直す。** 試験は状態を持つので、使い回すと前の試験が残した
プレイヤーやフェーズのせいで後の試験が落ち、原因の分かりにくい失敗になる。
この前提は `scripts/run_server_integration_tests.sh` にまとめてある。

登録は `"テスト名:再接続猶予秒"` の形。秒を空にするとサーバーの既定を使う。
`forfeit_test` と `cpu_orphan_test` は「切断で終わる」ことを確かめるので `1` を指定し、
既定の猶予を待たずに済ませている。

| テスト | 守っているもの |
| --- | --- |
| `network_test` | 2クライアントの通信と状態の伝わり方 |
| `reconnect_test` | 切断で試合が止まり、同じトークンで同じPlayerへ戻れる |
| `lobby_leave_test` | 退出が猶予を待たず次のSnapshotへ出る |
| `forfeit_test` | 残り1人になったときの決着 |
| `cpu_orphan_test` | 人間が居なくなったCPU戦が空のルームへ戻る |
| `sandbox_test` | 練習場が1人で成立する（的3体、全6種類、時間で終わらない） |
| `random_map_test` | RANDOMが一覧に出て、選ぶと届き、開始で作り直される |
| `cpu_level_test` | CPU1体ごとの強さと、自分の色の選択が届く |

Compose環境を起動しているとポート9001が埋まる。別のポートで走らせられる。

```bash
make integration-server INTEGRATION_TEST_PORT=9031 INTEGRATION_TEST_CONTROL_PORT=9131
```

`make integration` は別枠で、AdminServer・Matchmaker・GameServer をすべて起動した
Compose環境に対して2本走る。`control_plane_test` が制御面を、`room_browser_test` が
「一覧から選んだ部屋へ実際に入れること」を確かめる。後者は1台構成では成立しない。
選んだ部屋と違う所へ案内されていないかを見るのに、2台以上が要る。

### 実行していないもの

`debug_web_test` は登録から外してある。`/debug/api/health` の `read_only` と
デバッグ画面の見出し `Server Observer` を要求するが、どちらも `ff6c8e1` で失われた。
テストを現状へ合わせるか、サーバー側に戻すかは仕様の判断が要る（未決）。

**黙って飛ばすと「全部通った」ように見えるため、実行のたびに理由を表示する。**

## CI

Pull Request で `rust` / `frontend` / `web` / `integration` の4ジョブが並列に走り、
上のすべてを通る。ジョブの中身と必須チェックの設定は
[`development-flow.md`](development-flow.md#ciが見るもの) にある。

統合試験とGodotテストの「守らないと結果が信用できなくなる前提」は、散文ではなく
実行スクリプトへ入れてある。新しい試験を足すときは前提もそちらへ書く。

## テストを足すとき

**どこに書くか**は、壊れたときに何が起きるかで決める。

- ゲームの計算が変わる → game-core の単体テスト。決定的に書けるので、実際に何tickか
  進めて結果を見る
- サーバーとクライアントで前提がずれ得る → 契約テスト。数値なら
  `shared_limits_golden`、メッセージの形なら `wire_golden`
- 画面から操作できなくなる／読めなくなる → Godot テスト
- 繋いでみないと分からない → 統合試験

**書いたら壊して確かめる。** このリポジトリのテストは、書いたあとに対象をわざと壊して
落ちることを確認したうえで入れている。落ちないテストは、書いた本人にしか意味がない。
実際、これで「判定が片側しか見ていない」「待ち方が緩くてすり抜ける」といった穴が
何度も見つかっている。

**調整してよい値をテストで縛らない。** 見え方の好みやバランスの数値を期待値に
書き写すと、調整するたびに壊れていないのに落ちる。範囲や関係（「上の段の方が強い」
「選んでいる行の方が目立つ」）で書く。
