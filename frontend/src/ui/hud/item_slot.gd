extends Control

const DARK := Color("#020a05")
const WHITE := Color("#8fff9d")
const LALOKIN_POPPOS: Texture2D = preload(
	"res://assets/generated/actors/lalokinpoppos/lalokinpoppos.png"
)

var kind := ""
var charges := 0
var shield_hp := 0
var berserk_left := 0.0


func apply_player(player: Dictionary) -> void:
	var held = player.get("held_item")
	kind = str(held.get("kind", "")) if typeof(held) == TYPE_DICTIONARY else ""
	charges = int(held.get("charges", 0)) if typeof(held) == TYPE_DICTIONARY else 0
	shield_hp = int(player.get("shield_hp", 0))
	berserk_left = float(player.get("berserk_left", 0.0))
	queue_redraw()


func _draw() -> void:
	var terminal_font := get_theme_default_font()
	draw_rect(Rect2(0, 0, 92, 40), Color(0.002, 0.024, 0.01, 0.92))
	draw_rect(Rect2(1, 1, 38, 38), Color("#2d7840"), false, 2.0)
	if kind.is_empty():
		draw_string(terminal_font, Vector2(10, 24), "—", HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color("#315f3b"))
	elif kind == "larokin_poppos":
		draw_texture_rect(LALOKIN_POPPOS, Rect2(Vector2(6, 10), Vector2(28, 20)), false)
	else:
		_draw_icon(Vector2(20, 20), kind)
	var title := "EMPTY" if kind.is_empty() else _name(kind)
	draw_string(terminal_font, Vector2(43, 15), title, HORIZONTAL_ALIGNMENT_LEFT, 47, 8, WHITE)
	var hint := "SPACE"
	if kind == "dash": hint += " ×%d" % charges
	elif shield_hp > 0: hint = "SHIELD %d" % shield_hp
	elif berserk_left > 0.0: hint = "RAGE %.1f" % berserk_left
	draw_string(terminal_font, Vector2(43, 30), hint, HORIZONTAL_ALIGNMENT_LEFT, 47, 8, Color("#3b7a49"))


func _draw_icon(center: Vector2, item_kind: String) -> void:
	var color := _color(item_kind)
	if item_kind == "dash":
		draw_rect(Rect2(center + Vector2(-6, -8), Vector2(8, 12)), color)
		draw_rect(Rect2(center + Vector2(-5, 3), Vector2(13, 5)), color)
	elif item_kind == "shield":
		draw_colored_polygon(PackedVector2Array([center+Vector2(-8,-9),center+Vector2(8,-9),center+Vector2(7,3),center+Vector2(0,10),center+Vector2(-7,3)]), color)
	elif item_kind == "ghost":
		draw_circle(center + Vector2(0, -2), 8, color)
		draw_rect(Rect2(center + Vector2(-8, -2), Vector2(16, 9)), color)
		draw_circle(center + Vector2(-3, -3), 1.5, DARK)
		draw_circle(center + Vector2(3, -3), 1.5, DARK)
	elif item_kind == "berserk":
		draw_colored_polygon(PackedVector2Array([center+Vector2(-7,9),center+Vector2(-4,-1),center,center+Vector2(2,-10),center+Vector2(8,0),center+Vector2(6,9)]), color)
	else:
		draw_circle(center, 9, color)
		draw_rect(Rect2(center + Vector2(-5, -2), Vector2(3, 3)), DARK)
		draw_rect(Rect2(center + Vector2(2, -2), Vector2(3, 3)), DARK)


func _name(item_kind: String) -> String:
	match item_kind:
		"dash": return "DASH"
		"larokin_poppos": return "LAROKIN"
		"berserk": return "BERSERK"
		"shield": return "SHIELD"
		"ghost": return "GHOST"
		_: return item_kind.to_upper()


func _color(item_kind: String) -> Color:
	match item_kind:
		"dash": return Color("#65ff83")
		"larokin_poppos": return Color("#a4ff9f")
		"berserk": return Color("#8dff7c")
		"shield": return Color("#72e88a")
		"ghost": return Color("#b0ffae")
		_: return WHITE
