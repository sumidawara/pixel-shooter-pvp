# 開発フロー

## 基本

`main` は保護し、変更はブランチとPull Requestで入れる。マージ前に必ず全チェックが
緑になった瞬間が存在する状態を作るのが目的で、レビューの有無とは別の話である。

```text
ブランチを切る
  └─ 作業してcommit
      └─ push すると CI が走る
          └─ 緑になったら PR をマージ（マージは人間が行う）
```

## ローカルで通すもの

```sh
make verify
```

`fmt-check` → `lint` → `test` → `web-check` → `test-frontend` を順に実行する。
Compose環境を要する統合試験は含めていない。所要時間を1〜2分に保ち、
コミット前に気軽に回せるようにするため。結合はCIで見る。

## CIが見るもの

`.github/workflows/ci.yml` の4ジョブ。すべてPRの必須チェックにする。

| ジョブ | 内容 |
| --- | --- |
| `rust` | 整形、Clippy（警告はエラー扱い）、Workspace全テスト |
| `frontend` | Godotヘッドレステスト（`test-frontend-full`） |
| `web` | Adminデバッグ画面の型検査と、`dist` の陳腐化検査 |
| `integration` | 単体GameServerとCompose環境に対する統合試験 |

### 前提はスクリプトへ入れる

統合試験とGodotテストには、守らないと結果が信用できなくなる前提がある。
散文に書くと守られないので、実行スクリプトへ入れてある。

- `scripts/run_server_integration_tests.sh`
  試験ごとにサーバーを作り直す。1台を使い回すと前の試験が残した状態で落ちる。
  再接続猶予も試験ごとに必要な値が違う。
- `scripts/run_frontend_tests.sh`
  グローバルクラス名の解決のため毎回インポートする。`join_room_flow_test` が
  必要とする待受サーバーを立てる。合否は `quit()` の終了コードで判定し、
  実行時エラーで止まらないようタイムアウトを掛ける。

新しい試験を足すときは、必要な前提もここへ書く。

## 変更の種類ごとの手順

### サーバーのゲーム規則・通信フォーマットを変えた

クライアント予測と通信フォーマットはRustとGDScriptの2箇所に存在する。
Rust側を変えたらゴールデンを作り直し、Godot側を追従させる。

```sh
make update-goldens
make test-frontend
```

作り直したfixtureはコミットする。忘れると `rust` ジョブが落ちる。

### Adminデバッグ画面を変えた

`tools/debug-web/dist` はコミット済みで、AdminServerが `include_bytes!` で埋め込む。
ビルドを忘れると、配布される画面だけが古いまま残る。

```sh
make web-build
```

`web` ジョブの `web-dist-check` が差分を検出する。

### 画像アセットを変えた

Aseprite原本が正。書き出してコミットする。

```sh
make assets-build
```

書き出しに失敗してもPNG自体は生成されるため、透明なままコミットされうる
（実際に `lalokinpoppos.png` が alpha=7/255 で入っていた）。
`frontend/tests/sprite_assets_test.gd` が不透明な画素の有無を検査する。

## CIでは守れないもの

仕組みで閉じられない範囲を把握しておく。

1. **Aseprite原本とPNGの同期。**
   AsepriteはCIに置けないため、「原本を編集したのに書き出し忘れた」は検出できない。
   運用とレビューで守るしかない。

2. **実際の対戦の手触り。**
   契約テストは規則の一致を保証するが、遊べるかどうかは人が見るしかない。

3. **`server.json` の既定値。**
   `join_secret` が `development-only-secret`、`require_join_ticket` が `false` のまま。
   CIの守備範囲ではないので、サーバー起動時のガードで塞ぐべき事項として残っている。

4. **リリースビルド。**
   macOS版はmacOSランナーとGodotのエクスポートテンプレートが要る。
   タグ契機の別ワークフローとして分ける。

## 実行しない試験

`scripts/debug_web_test.mjs` は現在のサーバーでは成立しないため実行していない。
`/debug/api/health` の `read_only` と、デバッグ画面の見出し `Server Observer` を
要求するが、どちらも `ff6c8e1`（web arena map editor）で失われている。
テストを現状へ合わせるか、サーバー側へ戻すかは仕様の判断が要る。

`scripts/run_server_integration_tests.sh` は実行時に必ずこの未実行を表示する。
黙って飛ばすと「全部通った」ように見えるため。

## main の保護設定

GitHubのSettings → Branches → Add branch protection rule で `main` に対して設定する。

- Require a pull request before merging
- Require status checks to pass before merging
  - 必須にするチェック: `Rust（整形・lint・テスト）` / `Godotクライアント` /
    `Adminデバッグ画面` / `サービス間の結合`
  - Require branches to be up to date before merging
- Do not allow bypassing the above settings

force pushと履歴改変、protected branchへの直接pushは行わない。
