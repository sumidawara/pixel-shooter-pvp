extends Node2D

const ARENA_SIZE := Vector2(640, 360)
const PANEL := Color("#0d1119")
const WHITE := Color("#e9f1f7")
const TILEMAP_TEXTURE: Texture2D = preload("res://assets/art/tilemap.png")
const OBSTACLES := [Rect2(250, 85, 140, 28), Rect2(250, 247, 140, 28)]


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, ARENA_SIZE), PANEL)
	for y in range(0, 360, 32):
		for x in range(0, 640, 32):
			if int(x / 32 + y / 32) % 2 == 0:
				draw_rect(Rect2(x, y, 32, 32), Color("#101722"))
	for x in range(0, 641, 32):
		draw_line(Vector2(x, 0), Vector2(x, 360), Color("#1a222d"), 1.0)
	for y in range(0, 361, 32):
		draw_line(Vector2(0, y), Vector2(640, y), Color("#1a222d"), 1.0)
	draw_rect(Rect2(Vector2(1, 1), ARENA_SIZE - Vector2(2, 2)), WHITE, false, 2.0)
	for obstacle in OBSTACLES:
		draw_texture_rect_region(TILEMAP_TEXTURE, obstacle, Rect2(0, 0, 32, 32))
		draw_rect(obstacle.grow(-4), Color("#202834"))
