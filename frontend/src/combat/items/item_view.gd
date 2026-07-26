extends Node2D

const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")
const WHITE := Color("#e9f1f7")

@onready var points_label: Label = %PointsLabel

var animation_time := 0.0


func configure(points: int) -> void:
	points_label.text = "+%d" % points


func _process(delta: float) -> void:
	animation_time += delta
	queue_redraw()


func _draw() -> void:
	var bob := roundf(sin(animation_time * 4.0) * 2.0)
	var offset := Vector2(0.0, bob)
	var pulse := 1.0 if fmod(animation_time, 0.6) < 0.3 else 0.7
	draw_rect(Rect2(offset + Vector2(-8, -8), Vector2(16, 16)), Color(0, 0, 0, 0.65))
	draw_rect(Rect2(offset + Vector2(-6, -10), Vector2(12, 20)), MAGENTA * pulse)
	draw_rect(Rect2(offset + Vector2(-10, -6), Vector2(20, 12)), CYAN * pulse)
	draw_rect(Rect2(offset + Vector2(-5, -5), Vector2(10, 10)), WHITE)
	draw_rect(Rect2(offset + Vector2(-2, -2), Vector2(4, 4)), Color("#151b26"))
