extends Node2D

const PANEL := Color("#010703")
const FLOOR_ALT := Color("#031109")
const GRID := Color(0.10, 0.42, 0.19, 0.34)
const GRID_MAJOR := Color(0.18, 0.68, 0.29, 0.5)
const SOLID := Color(0.01, 0.055, 0.075, 0.92)
const SOLID_EDGE := Color("#27e5ff")
const DESTRUCTIBLE := Color(0.1, 0.045, 0.012, 0.92)
const DESTRUCTIBLE_EDGE := Color("#ff914d")

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
                    _draw_wireframe_cell(rect, SOLID, SOLID_EDGE, false)
                ArenaMapData.DESTRUCTIBLE_WALL:
                    _draw_wireframe_cell(rect, DESTRUCTIBLE, DESTRUCTIBLE_EDGE, true)

    for x in range(arena_map.width + 1):
        var color := GRID_MAJOR if x % 4 == 0 else GRID
        draw_line(Vector2(x * cell_size, 0), Vector2(x * cell_size, map_size.y), color)
    for y in range(arena_map.height + 1):
        var color := GRID_MAJOR if y % 4 == 0 else GRID
        draw_line(Vector2(0, y * cell_size), Vector2(map_size.x, y * cell_size), color)
    draw_rect(Rect2(Vector2.ONE, map_size - Vector2(2, 2)), SOLID_EDGE, false, 1.0)


func _draw_wireframe_cell(
    rect: Rect2,
    fill: Color,
    edge: Color,
    destructible: bool
) -> void:
    draw_rect(rect, fill)
    var frame := rect.grow(-2.0)
    draw_rect(frame, edge, false, 1.0)
    var inset := rect.grow(-7.0)
    draw_rect(inset, Color(edge, 0.42), false, 1.0)

    if destructible:
        draw_line(inset.position, inset.end, edge, 1.0)
        draw_line(
            Vector2(inset.end.x, inset.position.y),
            Vector2(inset.position.x, inset.end.y),
            edge,
            1.0
        )
    else:
        var center := rect.get_center()
        draw_line(Vector2(inset.position.x, center.y), Vector2(inset.end.x, center.y), Color(edge, 0.35))
        draw_line(Vector2(center.x, inset.position.y), Vector2(center.x, inset.end.y), Color(edge, 0.35))
