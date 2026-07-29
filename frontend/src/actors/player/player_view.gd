extends Node2D

const PLAYER_STAND: Texture2D = preload("res://assets/generated/actors/player/player_stand.png")
const PLAYER_RUN: Texture2D = preload("res://assets/generated/actors/player/player_run.png")

@onready var outline_sprite: Sprite2D = %OutlineSprite
@onready var character_sprite: Sprite2D = %CharacterSprite
@onready var aim_line: Line2D = %AimLine
@onready var dash_trail: Line2D = %DashTrail
@onready var name_label: Label = %NameLabel
@onready var disconnected_label: Label = %DisconnectedLabel
@onready var respawn_label: Label = %RespawnLabel

var state: Dictionary = {}
var accent_color := Color.WHITE
var moving := false


func apply_state(next_state: Dictionary, color: Color, is_moving: bool) -> void:
	state = next_state
	accent_color = color
	moving = is_moving
	name_label.text = str(state.get("name", "Player"))
	name_label.modulate = accent_color
	disconnected_label.visible = not bool(state.get("connected", true))
	disconnected_label.modulate = accent_color
	var alive := bool(state.get("alive", false))
	character_sprite.visible = alive
	outline_sprite.visible = alive
	aim_line.visible = alive
	dash_trail.visible = alive and bool(state.get("dashing", false))
	respawn_label.visible = not alive
	if not alive:
		respawn_label.text = "RESPAWN %.1f" % float(state.get("respawn_left", 0.0))
		respawn_label.modulate = accent_color
		return
	_update_sprite()
	var aim := _to_vector(state.get("aim", {}))
	aim_line.default_color = accent_color
	aim_line.points = PackedVector2Array([aim * 4.0, aim * 22.0])
	dash_trail.default_color = Color(accent_color, 0.35)
	dash_trail.points = PackedVector2Array([-aim * 8.0, -aim * 28.0])


func _process(_delta: float) -> void:
	if state.is_empty() or not bool(state.get("alive", false)):
		return
	var invulnerable := float(state.get("invulnerable_left", 0.0)) > 0.0
	var visible_now := not invulnerable or Time.get_ticks_msec() % 120 < 65
	character_sprite.visible = visible_now
	outline_sprite.visible = visible_now
	aim_line.visible = visible_now
	if moving:
		_update_sprite()


func _update_sprite() -> void:
	var texture := PLAYER_RUN if moving else PLAYER_STAND
	var frame := int(Time.get_ticks_msec() / 95) % 4
	var region := Rect2(frame * 32, 0, 32, 32) if moving else Rect2(0, 0, 32, 32)
	for sprite in [outline_sprite, character_sprite]:
		sprite.texture = texture
		sprite.region_enabled = true
		sprite.region_rect = region
	outline_sprite.modulate = accent_color


func _to_vector(value: Dictionary) -> Vector2:
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))
