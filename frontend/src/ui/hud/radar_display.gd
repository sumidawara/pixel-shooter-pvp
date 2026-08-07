extends Control

const PHOSPHOR := Color("#78ff8f")
const PHOSPHOR_DIM := Color(0.24, 0.78, 0.38, 0.42)
const PANEL := Color(0.005, 0.03, 0.014, 0.88)
const MAP_SIZE := Vector2(640.0, 360.0)

var players: Array = []
var local_player_id := 0
var sweep_angle := 0.0


func _ready() -> void:
    mouse_filter = Control.MOUSE_FILTER_IGNORE
    set_process(true)


func apply_snapshot(next_players: Array, next_local_player_id: int) -> void:
    players = next_players.duplicate(true)
    local_player_id = next_local_player_id
    queue_redraw()


func _process(delta: float) -> void:
    sweep_angle = fmod(sweep_angle + delta * 0.9, TAU)
    queue_redraw()


func _draw() -> void:
    draw_rect(Rect2(Vector2.ZERO, size), PANEL)
    draw_rect(Rect2(Vector2.ONE, size - Vector2(2, 2)), PHOSPHOR_DIM, false, 1.0)

    var center := Vector2(size.x * 0.5, size.y * 0.48)
    var radius := minf(size.x, size.y) * 0.36
    for ring in range(1, 4):
        draw_arc(center, radius * float(ring) / 3.0, 0.0, TAU, 32, PHOSPHOR_DIM, 1.0)
    draw_line(center - Vector2(radius, 0), center + Vector2(radius, 0), PHOSPHOR_DIM)
    draw_line(center - Vector2(0, radius), center + Vector2(0, radius), PHOSPHOR_DIM)
    draw_line(center, center + Vector2.RIGHT.rotated(sweep_angle) * radius, PHOSPHOR_DIM)

    var local_position := MAP_SIZE * 0.5
    for player in players:
        if int(player.get("id", 0)) == local_player_id:
            local_position = _to_vector(player.get("position", {}))
            break

    var local_marker := PackedVector2Array([
        center + Vector2(0, -4),
        center + Vector2(-3, 3),
        center + Vector2(3, 3),
    ])
    draw_colored_polygon(local_marker, PHOSPHOR)

    for player in players:
        var id := int(player.get("id", 0))
        if id == local_player_id or not bool(player.get("alive", false)):
            continue
        var relative := _to_vector(player.get("position", {})) - local_position
        var marker := center + Vector2(
            relative.x / MAP_SIZE.x,
            relative.y / MAP_SIZE.y
        ) * radius * 2.0
        var from_center := marker - center
        if from_center.length() > radius:
            marker = center + from_center.normalized() * radius
        var alpha := 0.95 if bool(player.get("connected", false)) else 0.35
        draw_circle(marker, 2.0, Color(PHOSPHOR, alpha))

    draw_string(
        get_theme_default_font(),
        Vector2(6, size.y - 4),
        "TAC/RADAR",
        HORIZONTAL_ALIGNMENT_LEFT,
        -1,
        7,
        PHOSPHOR_DIM
    )


func _to_vector(value: Dictionary) -> Vector2:
    return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))
