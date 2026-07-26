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

## 効果音

`frontend/src/shared/audio/` の効果音は本作向けに生成した短い波形です。
再生成するときはリポジトリ直下で次を実行します。

```sh
node scripts/generate_sfx.mjs
```
