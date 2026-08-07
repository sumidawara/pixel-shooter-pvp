extends Control

const PANEL_COLOR := Color("#09111b")
const PANEL_BORDER := Color("#344252")
const MUTED_COLOR := Color("#8091a3")
const TRACK_COLOR := Color("#283542")
const READY_COLOR := Color("#67efa2")

@onready var panel: Panel = %Panel
@onready var accent: ColorRect = %Accent
@onready var rank_label: Label = %RankLabel
@onready var avatar_outline: TextureRect = %AvatarOutline
@onready var avatar: TextureRect = %Avatar
@onready var name_label: Label = %NameLabel
@onready var score_label: Label = %ScoreLabel
@onready var hp_key_label: Label = %HpKeyLabel
@onready var hp_track: ColorRect = %HpTrack
@onready var hp_fill: ColorRect = %HpFill
@onready var hp_value_label: Label = %HpValueLabel
@onready var ammo_label: Label = %AmmoLabel
@onready var dash_label: Label = %DashLabel
@onready var dash_track: ColorRect = %DashTrack
@onready var dash_fill: ColorRect = %DashFill


func apply_player(
	player: Dictionary,
	color: Color,
	dash_cooldown: float,
	rank: int,
	is_local: bool
) -> void:
	visible = not player.is_empty()
	if player.is_empty():
		return
	_configure_layout(is_local)
	_apply_panel_style(color, is_local)

	var display_name := str(player.get("name", "P"))
	if bool(player.get("is_cpu", false)):
		display_name += "*"
	name_label.text = display_name
	rank_label.text = "#%d" % rank
	rank_label.modulate = color
	score_label.text = "%dP" % int(player.get("score", 0))
	avatar_outline.modulate = color

	var hp := int(player.get("hp", 0))
	var max_hp := maxi(int(player.get("max_hp", 1)), 1)
	var hp_ratio := clampf(float(hp) / float(max_hp), 0.0, 1.0)
	hp_fill.color = color
	hp_fill.size.x = hp_track.size.x * hp_ratio
	hp_value_label.text = "%d/%d" % [hp, max_hp]

	var ammo := int(player.get("ammo", 0))
	var max_ammo := int(player.get("max_ammo", 6))
	ammo_label.text = "AMMO %d/%d" % [ammo, max_ammo] if is_local else "A %d" % ammo

	var cooldown_left := maxf(float(player.get("dash_cooldown_left", 0.0)), 0.0)
	var dash_ratio := 1.0 - clampf(cooldown_left / maxf(dash_cooldown, 0.01), 0.0, 1.0)
	dash_fill.color = READY_COLOR if cooldown_left <= 0.0 else color
	dash_fill.size.x = dash_track.size.x * dash_ratio
	if is_local:
		dash_label.text = "DASH READY" if cooldown_left <= 0.0 else "DASH %.1f" % cooldown_left
	else:
		dash_label.text = "D+" if cooldown_left <= 0.0 else "D%.1f" % cooldown_left
		dash_label.modulate = READY_COLOR if cooldown_left <= 0.0 else MUTED_COLOR


func _configure_layout(is_local: bool) -> void:
	var width := size.x
	accent.position = Vector2.ZERO
	accent.size = Vector2(2.0 if is_local else 1.0, size.y)
	rank_label.position = Vector2(4.0, 1.0)
	rank_label.size = Vector2(25.0 if is_local else 22.0, 35.0)
	rank_label.add_theme_font_size_override("font_size", 15 if is_local else 12)

	if is_local:
		avatar_outline.position = Vector2(31.0, 3.0)
		avatar_outline.size = Vector2(34.0, 34.0)
		avatar.position = Vector2(33.0, 5.0)
		avatar.size = Vector2(30.0, 30.0)
		name_label.position = Vector2(69.0, 0.0)
		name_label.size = Vector2(width - 136.0, 13.0)
		name_label.add_theme_font_size_override("font_size", 9)
		score_label.position = Vector2(width - 64.0, 0.0)
		score_label.size = Vector2(59.0, 13.0)
		score_label.add_theme_font_size_override("font_size", 9)
		hp_key_label.visible = true
		hp_key_label.position = Vector2(69.0, 13.0)
		hp_key_label.size = Vector2(19.0, 11.0)
		hp_track.position = Vector2(89.0, 16.0)
		hp_track.size = Vector2(width - 151.0, 6.0)
		hp_value_label.visible = true
		hp_value_label.position = Vector2(width - 58.0, 12.0)
		hp_value_label.size = Vector2(53.0, 12.0)
		ammo_label.position = Vector2(69.0, 25.0)
		ammo_label.size = Vector2(69.0, 11.0)
		ammo_label.add_theme_font_size_override("font_size", 8)
		dash_label.position = Vector2(141.0, 25.0)
		dash_label.size = Vector2(width - 146.0, 11.0)
		dash_label.add_theme_font_size_override("font_size", 8)
		dash_track.visible = true
		dash_track.position = Vector2(141.0, 35.0)
		dash_track.size = Vector2(width - 146.0, 2.0)
	else:
		avatar_outline.position = Vector2(26.0, 7.0)
		avatar_outline.size = Vector2(26.0, 26.0)
		avatar.position = Vector2(28.0, 9.0)
		avatar.size = Vector2(22.0, 22.0)
		name_label.position = Vector2(55.0, 0.0)
		name_label.size = Vector2(width - 59.0, 12.0)
		name_label.add_theme_font_size_override("font_size", 8)
		score_label.position = Vector2(55.0, 11.0)
		score_label.size = Vector2(43.0, 11.0)
		score_label.add_theme_font_size_override("font_size", 8)
		hp_key_label.visible = false
		hp_track.position = Vector2(55.0, 25.0)
		hp_track.size = Vector2(width - 59.0, 5.0)
		hp_value_label.visible = false
		ammo_label.position = Vector2(55.0, 29.0)
		ammo_label.size = Vector2(34.0, 9.0)
		ammo_label.add_theme_font_size_override("font_size", 7)
		dash_label.position = Vector2(width - 38.0, 29.0)
		dash_label.size = Vector2(34.0, 9.0)
		dash_label.add_theme_font_size_override("font_size", 7)
		dash_track.visible = false

	# HPの幅変更後に、次の状態反映で割合が正しく適用されるようトラックを初期化する。
	hp_fill.position = Vector2.ZERO
	hp_fill.size.y = hp_track.size.y
	dash_fill.position = Vector2.ZERO
	dash_fill.size.y = dash_track.size.y


func _apply_panel_style(color: Color, is_local: bool) -> void:
	accent.color = color
	var style := StyleBoxFlat.new()
	style.bg_color = PANEL_COLOR
	style.border_color = color if is_local else PANEL_BORDER
	var border_width := 2 if is_local else 1
	style.set_border_width_all(border_width)
	panel.add_theme_stylebox_override("panel", style)
