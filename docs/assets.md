# アセット

## 画像・フォント

画像とピクセルフォントは、ゲーム制作者本人が制作した
[`sumidawara/pixel_shooter`](https://github.com/sumidawara/pixel_shooter)
の素材を本作向けに選んで使用しています。

- `front/assets/art/player_stand.png`
- `front/assets/art/player_run.png`
- `front/assets/art/cursor.png`
- `front/assets/art/sparkle.png`
- `front/assets/art/tilemap.png`
- `front/assets/art/title.png`
- `front/assets/art/gameover.png`
- `front/assets/fonts/PixelMplus12-Regular.ttf`
- `front/assets/fonts/PixelMplus12-Bold.ttf`

元の素材は `ri-rehi/App/resources/` 以下にあります。本作ではレベル制度や
ダンジョン要素は引き継がず、モノトーンのドット絵とUI表現だけを利用します。

## 効果音

`front/assets/audio/` の効果音は本作向けに生成した短い波形です。
再生成するときはリポジトリ直下で次を実行します。

```sh
node scripts/generate_sfx.mjs
```
