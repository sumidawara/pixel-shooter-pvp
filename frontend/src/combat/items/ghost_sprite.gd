@tool
extends RefCounted

## Ghostの原画と、コマ切り出しの共通処理。
##
## フィールド上のアイテム、HUDのスロット、奪取演出の3箇所が同じ絵を使う。
## コマの割り方を各所に書くと、原画のコマ数を変えたときに直し漏れる。

const TEXTURE: Texture2D = preload("res://assets/aseprite/actors/ghost/ghost.aseprite")
const FRAME_SIZE := Vector2(32.0, 32.0)
const FRAME_COUNT := 2
## 1秒あたりのコマ送り数。ふわふわ漂う速さ。
const FRAMES_PER_SECOND := 3.0


## 経過時間から表示するコマを選ぶ。
static func frame_at(seconds: float) -> int:
	return int(seconds * FRAMES_PER_SECOND) % FRAME_COUNT


static func region(frame_index: int) -> Rect2:
	return Rect2(Vector2(FRAME_SIZE.x * frame_index, 0.0), FRAME_SIZE)


## 指定した矩形へ1コマ描く。
static func draw_frame(
	canvas: CanvasItem,
	destination: Rect2,
	seconds: float,
	modulate: Color = Color.WHITE
) -> void:
	canvas.draw_texture_rect_region(
		TEXTURE, destination, region(frame_at(seconds)), modulate)
