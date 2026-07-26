extends Node2D

const PANEL := Color("#0d1119")
const FLOOR_ALT := Color("#101722")
const GRID := Color("#1a222d")
const SOLID := Color("#202834")
const SOLID_EDGE := Color("#e9f1f7")
const DESTRUCTIBLE := Color("#70452f")
const DESTRUCTIBLE_EDGE := Color("#d99a62")

var arena_map: ArenaMapData


func set_arena_map(map: ArenaMapData) -> void:
	arena_map = map
	queue_redraw()


func _draw() -> void:
	if arena_map == null:
		return
	var map_size := arena_map.size_pixels()
	var cell_size := float(arena_map.tile_size)
	draw_rect(Rect2(Vector2.ZERO, map_size), PANEL)
	for y in range(arena_map.height):
		for x in range(arena_map.width):
			var cell := Vector2i(x, y)
			var rect := Rect2(Vector2(x, y) * cell_size, Vector2.ONE * cell_size)
			if (x + y) % 2 == 0:
				draw_rect(rect, FLOOR_ALT)
			match arena_map.tile_at(cell):
				ArenaMapData.SOLID_WALL:
					draw_rect(rect, SOLID_EDGE)
					draw_rect(rect.grow(-3.0), SOLID)
				ArenaMapData.DESTRUCTIBLE_WALL:
					draw_rect(rect, DESTRUCTIBLE_EDGE)
					draw_rect(rect.grow(-3.0), DESTRUCTIBLE)
					draw_line(rect.position + Vector2(7, 7), rect.end - Vector2(7, 7), DESTRUCTIBLE_EDGE, 2)
					draw_line(
						Vector2(rect.end.x - 7, rect.position.y + 7),
						Vector2(rect.position.x + 7, rect.end.y - 7),
						DESTRUCTIBLE_EDGE,
						2
					)
	for x in range(arena_map.width + 1):
		draw_line(Vector2(x * cell_size, 0), Vector2(x * cell_size, map_size.y), GRID)
	for y in range(arena_map.height + 1):
		draw_line(Vector2(0, y * cell_size), Vector2(map_size.x, y * cell_size), GRID)
