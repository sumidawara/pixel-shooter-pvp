extends Node2D

const WHITE := Color("#e9f1f7")
const DARK := Color("#080c12")
const LALOKIN_POPPOS: Texture2D = preload(
	"res://assets/generated/actors/lalokinpoppos/lalokinpoppos.png"
)

@onready var points_label: Label = %PointsLabel

var animation_time := 0.0
var kind := "energy_cell"


func configure(next_kind: String, points: int) -> void:
	kind = next_kind
	points_label.text = "+%d" % points if kind == "energy_cell" else _item_name(kind)


func _process(delta: float) -> void:
	animation_time += delta
	queue_redraw()


func _draw() -> void:
	var bob := roundf(sin(animation_time * 4.0) * 2.0)
	var offset := Vector2(0.0, bob)
	var pulse := 1.0 if fmod(animation_time, 0.6) < 0.3 else 0.7
	draw_rect(Rect2(offset + Vector2(-9, -9), Vector2(18, 18)), Color(0, 0, 0, 0.72))
	var color := _item_color(kind) * pulse
	if kind == "energy_cell":
		draw_rect(Rect2(offset + Vector2(-4, -10), Vector2(8, 20)), color)
		draw_rect(Rect2(offset + Vector2(-10, -4), Vector2(20, 8)), color)
	elif kind == "dash":
		draw_rect(Rect2(offset + Vector2(-6, -7), Vector2(8, 11)), color)
		draw_rect(Rect2(offset + Vector2(-5, 3), Vector2(13, 5)), color)
	elif kind == "shield":
		draw_colored_polygon(PackedVector2Array([offset+Vector2(-8,-8),offset+Vector2(8,-8),offset+Vector2(7,3),offset+Vector2(0,10),offset+Vector2(-7,3)]), color)
	elif kind == "ghost":
		draw_circle(offset + Vector2(0, -2), 8, color)
		draw_rect(Rect2(offset + Vector2(-8, -2), Vector2(16, 9)), color)
		draw_circle(offset + Vector2(-3, -3), 1.5, DARK)
		draw_circle(offset + Vector2(3, -3), 1.5, DARK)
	elif kind == "berserk":
		draw_colored_polygon(PackedVector2Array([offset+Vector2(-7,8),offset+Vector2(-4,-2),offset,offset+Vector2(2,-10),offset+Vector2(8,0),offset+Vector2(6,8)]), color)
	elif kind == "larokin_poppos":
		draw_texture_rect(LALOKIN_POPPOS, Rect2(offset + Vector2(-12, -9), Vector2(24, 18)), false)
	else:
		draw_circle(offset, 9, color)
		draw_rect(Rect2(offset + Vector2(-5, -2), Vector2(3, 3)), DARK)
		draw_rect(Rect2(offset + Vector2(2, -2), Vector2(3, 3)), DARK)
	if kind != "ghost" and kind != "larokin_poppos":
		draw_rect(Rect2(offset + Vector2(-2, -2), Vector2(4, 4)), WHITE)


func _item_color(item_kind: String) -> Color:
	match item_kind:
		"dash": return Color("#6688ff")
		"larokin_poppos": return Color("#ff914d")
		"berserk": return Color("#ff4f5e")
		"shield": return Color("#a879ff")
		"ghost": return Color("#c7a7ff")
		_: return WHITE


func _item_name(item_kind: String) -> String:
	match item_kind:
		"dash": return "DASH"
		"larokin_poppos": return "LAROKIN"
		"berserk": return "BERSERK"
		"shield": return "SHIELD"
		"ghost": return "GHOST"
		_: return "CELL"
