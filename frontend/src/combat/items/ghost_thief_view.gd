extends Node2D

## Ghost使用時の奪取演出。
##
## 使用者から対象へ飛び、アイテムを掴んで戻ってくる。
## 「誰が誰から何を奪ったか」と進み具合はサーバーが決め、ここは描き方だけを持つ。
## 奪取そのものは使用したtickで確定しているため、この表示は結果に影響しない。

const GhostSprite := preload("res://src/combat/items/ghost_sprite.gd")

const DARK := Color("#080c12")
const GHOST_TINT := Color("#dcccff")
const SIZE := Vector2(26.0, 26.0)
## 往路と復路の折り返し地点。ここで対象へ到達し、アイテムを掴む。
const TURNING_POINT := 0.5
## 弧の高さ。直線だと移動に見えず、掠め取った感じが出ない。
const ARC_HEIGHT := 26.0

var from := Vector2.ZERO
var to := Vector2.ZERO
var progress := 0.0
var stolen_kind := ""
var accent := Color.WHITE

var _elapsed := 0.0


func apply_state(state: Dictionary, owner_color: Color) -> void:
	from = _to_vector(state.get("from", {}))
	to = _to_vector(state.get("to", {}))
	progress = clampf(float(state.get("progress", 0.0)), 0.0, 1.0)
	stolen_kind = str(state.get("stolen_kind", ""))
	accent = owner_color
	queue_redraw()


func _process(delta: float) -> void:
	_elapsed += delta
	# 進み具合はスナップショット間隔(20Hz)でしか更新されないため、
	# コマ送りと明滅だけは描画フレームごとに進める。
	queue_redraw()


## 往路で対象へ向かい、折り返して使用者へ戻る軌道。
##
## progress 0.0 で使用者、TURNING_POINT で対象、1.0 で使用者へ戻る。
## 描画から切り離してあるのは、この往復こそが演出の中身であり、
## ヘッドレスでも検証できるようにするため。
static func flight_position(
	origin: Vector2, destination: Vector2, at_progress: float
) -> Vector2:
	var clamped := clampf(at_progress, 0.0, 1.0)
	var outward := clamped <= TURNING_POINT
	# 往路は origin→destination、復路は destination→origin。
	# それぞれの区間で 0→1 になるように正規化する。
	var leg := (
		clamped / TURNING_POINT
		if outward
		else (clamped - TURNING_POINT) / (1.0 - TURNING_POINT)
	)
	var start := origin if outward else destination
	var goal := destination if outward else origin
	var point := start.lerp(goal, leg)
	# 弧を描いて浮かせる。往路と復路で反対側へ膨らませ、往復が分かるようにする。
	point.y -= sin(leg * PI) * ARC_HEIGHT * (1.0 if outward else -1.0)
	return point


func _draw() -> void:
	var outward := progress <= TURNING_POINT
	var center := flight_position(from, to, progress)

	# 出現直後と消える直前は薄くして、唐突に現れ消えないようにする。
	var fade := clampf(minf(progress, 1.0 - progress) / 0.15, 0.0, 1.0)
	# テレサらしく半透明で、ゆらゆら明滅させる。
	var flicker := 0.72 + 0.14 * sin(_elapsed * 9.0)
	var alpha := fade * flicker

	# 対象を掴んだ瞬間の閃光。
	if absf(progress - TURNING_POINT) < 0.06:
		draw_circle(to - position, 15.0, Color(accent.r, accent.g, accent.b, 0.35 * fade))

	var destination := Rect2(center - position - SIZE * 0.5, SIZE)
	# 白い原画が明るい床で消えないよう、影を1px下へ重ねる。
	GhostSprite.draw_frame(
		self,
		Rect2(destination.position + Vector2(1, 1), destination.size),
		_elapsed,
		Color(DARK.r, DARK.g, DARK.b, alpha * 0.85))
	GhostSprite.draw_frame(
		self,
		destination,
		_elapsed,
		Color(GHOST_TINT.r, GHOST_TINT.g, GHOST_TINT.b, alpha))

	# 復路では奪ったアイテムを抱えて運ぶ。何が盗られたのか見て分かるように。
	if not outward:
		var carried := destination.position + Vector2(SIZE.x * 0.5, SIZE.y) + Vector2(0, 2)
		draw_circle(carried, 5.0, Color(0, 0, 0, 0.55 * alpha))
		draw_circle(carried, 4.0, Color(
			_item_color(stolen_kind).r,
			_item_color(stolen_kind).g,
			_item_color(stolen_kind).b,
			alpha))


func _item_color(item_kind: String) -> Color:
	match item_kind:
		"dash": return Color("#6688ff")
		"larokin_poppos": return Color("#ff914d")
		"berserk": return Color("#ff4f5e")
		"shield": return Color("#a879ff")
		"ghost": return Color("#c7a7ff")
		_: return Color("#e9f1f7")


func _to_vector(value) -> Vector2:
	if typeof(value) != TYPE_DICTIONARY:
		return Vector2.ZERO
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))
