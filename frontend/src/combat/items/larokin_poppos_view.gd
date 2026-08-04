extends Node2D

var telegraph_left := 0.0
var velocity := Vector2.ZERO


func apply_state(state: Dictionary) -> void:
	telegraph_left = float(state.get("telegraph_left", 0.0))
	var raw: Dictionary = state.get("velocity", {})
	velocity = Vector2(float(raw.get("x", 0.0)), float(raw.get("y", 0.0)))
	rotation = velocity.angle()
	queue_redraw()


func _draw() -> void:
	var blink := telegraph_left > 0.0 and Time.get_ticks_msec() % 140 < 70
	if blink:
		draw_circle(Vector2.ZERO, 12.0, Color(1.0, 0.32, 0.24, 0.22))
		draw_arc(Vector2.ZERO, 12.0, 0.0, TAU, 16, Color("#ff645e"), 2.0)
	var body := Color("#ff914d") if telegraph_left <= 0.0 else Color("#e9f1f7")
	draw_rect(Rect2(-7, -6, 14, 12), Color("#080c12"))
	draw_rect(Rect2(-5, -4, 10, 8), body)
	draw_rect(Rect2(0, -2, 2, 2), Color("#080c12"))
	draw_colored_polygon(PackedVector2Array([Vector2(-7, 0), Vector2(-12, -5), Vector2(-12, 5)]), body)
