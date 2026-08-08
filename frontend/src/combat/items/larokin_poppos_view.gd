extends Node2D

const LALOKIN_POPPOS: Texture2D = preload(
	"res://assets/aseprite/actors/lalokinpoppos/lalokinpoppos.aseprite"
)

var telegraph_left := 0.0
var velocity := Vector2.ZERO


func apply_state(state: Dictionary) -> void:
	telegraph_left = float(state.get("telegraph_left", 0.0))
	var raw: Dictionary = state.get("velocity", {})
	velocity = Vector2(float(raw.get("x", 0.0)), float(raw.get("y", 0.0)))
	# 原画は左向きなので、進行方向へ頭を向けるため半回転を加える。
	rotation = velocity.angle() + PI
	queue_redraw()


func _draw() -> void:
	var blink := telegraph_left > 0.0 and Time.get_ticks_msec() % 140 < 70
	if blink:
		draw_circle(Vector2.ZERO, 20.0, Color(1.0, 0.32, 0.24, 0.25))
		draw_arc(Vector2.ZERO, 20.0, 0.0, TAU, 20, Color("#ff645e"), 2.0)
	# 白い原画が壁へ重なっても消えないよう、暗い影の上へ拡大して直接描画する。
	draw_texture_rect(
		LALOKIN_POPPOS,
		Rect2(Vector2(-19, -13), Vector2(40, 29)),
		false,
		Color("#080c12")
	)
	draw_texture_rect(
		LALOKIN_POPPOS,
		Rect2(Vector2(-21, -15), Vector2(40, 29)),
		false
	)
