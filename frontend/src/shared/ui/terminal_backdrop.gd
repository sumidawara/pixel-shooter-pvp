extends Control

const PHOSPHOR := Color(0.33, 1.0, 0.48, 0.18)
const PHOSPHOR_DIM := Color(0.16, 0.62, 0.28, 0.12)
const PHOSPHOR_FAINT := Color(0.12, 0.4, 0.2, 0.075)
const VOID := Color("#020805")

var phase := 0.0


func _ready() -> void:
    mouse_filter = Control.MOUSE_FILTER_IGNORE
    set_process(true)


func _process(delta: float) -> void:
    phase = fmod(phase + delta, TAU)
    queue_redraw()


func _draw() -> void:
    draw_rect(Rect2(Vector2.ZERO, size), VOID)
    _draw_orthographic_grid()
    _draw_perspective_floor()
    _draw_radar()
    _draw_signal_trace()
    _draw_corner_marks()


func _draw_orthographic_grid() -> void:
    for x in range(0, int(size.x) + 1, 32):
        draw_line(Vector2(x, 0), Vector2(x, size.y), PHOSPHOR_FAINT)
    for y in range(0, int(size.y) + 1, 32):
        draw_line(Vector2(0, y), Vector2(size.x, y), PHOSPHOR_FAINT)


func _draw_perspective_floor() -> void:
    var horizon := size.y * 0.56
    var vanishing_point := Vector2(size.x * 0.51, horizon)
    draw_line(Vector2(0, horizon), Vector2(size.x, horizon), PHOSPHOR_DIM, 1.0)
    for index in range(-10, 11):
        var bottom_x := size.x * 0.5 + float(index) * 52.0
        draw_line(vanishing_point, Vector2(bottom_x, size.y), PHOSPHOR_DIM, 1.0)
    for index in range(1, 10):
        var ratio := pow(float(index) / 9.0, 1.7)
        var y := lerpf(horizon, size.y, ratio)
        draw_line(Vector2(0, y), Vector2(size.x, y), PHOSPHOR_DIM, 1.0)

    var blocks := [
        Rect2(255, horizon - 36, 52, 36),
        Rect2(323, horizon - 58, 66, 58),
        Rect2(405, horizon - 25, 82, 25),
    ]
    for block in blocks:
        draw_rect(block, Color(0.02, 0.12, 0.055, 0.34))
        draw_rect(block, PHOSPHOR, false, 1.0)
        draw_line(block.position, block.end, PHOSPHOR_FAINT)
        draw_line(
            Vector2(block.end.x, block.position.y),
            Vector2(block.position.x, block.end.y),
            PHOSPHOR_FAINT
        )


func _draw_radar() -> void:
    var center := Vector2(size.x - 108.0, 224.0)
    var radius := 62.0
    for ring in range(1, 4):
        draw_arc(center, radius * float(ring) / 3.0, 0.0, TAU, 48, PHOSPHOR_DIM, 1.0)
    draw_line(center - Vector2(radius, 0), center + Vector2(radius, 0), PHOSPHOR_DIM)
    draw_line(center - Vector2(0, radius), center + Vector2(0, radius), PHOSPHOR_DIM)
    var sweep_angle := phase * 0.42
    draw_line(center, center + Vector2.RIGHT.rotated(sweep_angle) * radius, PHOSPHOR, 1.0)
    for index in range(3):
        var angle := phase * (0.16 + index * 0.04) + float(index) * 2.1
        var distance := 20.0 + float(index) * 13.0
        draw_circle(center + Vector2.RIGHT.rotated(angle) * distance, 2.0, PHOSPHOR)


func _draw_signal_trace() -> void:
    var origin := Vector2(20.0, size.y - 25.0)
    var points := PackedVector2Array()
    for index in range(70):
        var x := origin.x + float(index) * 3.0
        var pulse := sin(float(index) * 0.48 + phase * 2.0)
        pulse += sin(float(index) * 0.13 - phase) * 0.35
        points.append(Vector2(x, origin.y + pulse * 5.0))
    if points.size() > 1:
        draw_polyline(points, PHOSPHOR, 1.0)


func _draw_corner_marks() -> void:
    var length := 18.0
    var inset := 8.0
    var corners := [
        Vector2(inset, inset),
        Vector2(size.x - inset, inset),
        Vector2(inset, size.y - inset),
        Vector2(size.x - inset, size.y - inset),
    ]
    for index in range(corners.size()):
        var corner: Vector2 = corners[index]
        var horizontal_sign := 1.0 if index in [0, 2] else -1.0
        var vertical_sign := 1.0 if index in [0, 1] else -1.0
        draw_line(corner, corner + Vector2(length * horizontal_sign, 0), PHOSPHOR)
        draw_line(corner, corner + Vector2(0, length * vertical_sign), PHOSPHOR)
