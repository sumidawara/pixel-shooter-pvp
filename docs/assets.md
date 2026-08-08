# アセット

## 画像・フォント

画像とピクセルフォントは、ゲーム制作者本人が制作した
[`sumidawara/pixel_shooter`](https://github.com/sumidawara/pixel_shooter)
の素材を本作向けに選んで使用しています。

- `frontend/src/actors/player/player_stand.png`
- `frontend/src/actors/player/player_run.png`
- `frontend/src/ui/menu/cursor.png`
- `frontend/src/game_modes/match/sparkle.png`
- `frontend/src/ui/menu/title.png`
- `frontend/src/ui/hud/gameover.png`
- `frontend/src/shared/fonts/PixelMplus12-Regular.ttf`
- `frontend/src/shared/fonts/PixelMplus12-Bold.ttf`

元の素材は `ri-rehi/App/resources/` 以下にあります。本作ではレベル制度や
ダンジョン要素は引き継がず、モノトーンのドット絵とUI表現だけを利用します。

## 読み込み

`frontend/assets/aseprite/` のAseprite原本が唯一の管理対象で、書き出し済みPNGは
持たない。`frontend/addons/aseprite_importer` が `.aseprite` をそのまま
テクスチャとして読み込むため、原本を保存すればGodotが取り込む。

```gdscript
preload("res://assets/aseprite/actors/player/player_stand.aseprite")
```

以前は原本とPNGの両方をコミットしていたが、どちらが正しいかを保証する仕組みが
無く、事故が起きた。書き出しに失敗してもPNG自体は生成されるため、
`lalokinpoppos.png` が alpha=7/255（ほぼ透明）のまま入っていたことがある。
また `ghost.aseprite` のように、原本だけ追加されて書き出されていないものもあった。
原本1つに寄せることで、この状態が構造的に発生しなくなる。

インポータはAsepriteの実行ファイルを呼ばず、GDScriptで形式を解釈する。
そのためCIでも同じ絵が再現でき、Asepriteは作画時にしか要らない。

対応しているのはこのプロジェクトが使う範囲（32bit RGBA、Normalブレンド、
セル種別 raw/linked/compressed）に限る。未対応の機能に出会ったら、
違う絵を出さずにインポートを失敗させる。検査は次の2つ。

- `frontend/tests/aseprite_document_test.gd`: 解析結果と、未対応機能を
  黙って読まないこと
- `frontend/tests/sprite_assets_test.gd`: 全原本がテクスチャとして読め、
  不透明な画素があること

## 効果音

`frontend/src/shared/audio/` の効果音は本作向けに生成した短い波形です。
再生成するときはリポジトリ直下で次を実行します。

```sh
node scripts/generate_sfx.mjs
```
