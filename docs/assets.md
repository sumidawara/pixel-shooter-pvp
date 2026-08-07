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

## 書き出し

`frontend/assets/aseprite/` のAseprite原本が正であり、
`frontend/assets/generated/` のPNGは `make assets-build` で書き出す。
原本を編集したら書き出し直してコミットする。

書き出しに失敗してもPNG自体は生成されるため、透明なまま気付かずに
コミットされうる（実際に lalokinpoppos.png が alpha=7/255 で入っていた）。
`frontend/tests/sprite_assets_test.gd` が生成物に不透明な画素があるかを
検査するので、`make test-frontend` を通しておくこと。

## 効果音

`frontend/src/shared/audio/` の効果音は本作向けに生成した短い波形です。
再生成するときはリポジトリ直下で次を実行します。

```sh
node scripts/generate_sfx.mjs
```
