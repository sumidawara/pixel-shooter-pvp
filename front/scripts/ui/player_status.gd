extends Control

@onready var info_label: Label = %InfoLabel
@onready var hp_container: HBoxContainer = %HpContainer
@onready var dash_bar: ProgressBar = %DashBar


func apply_player(player: Dictionary, color: Color, dash_cooldown: float) -> void:
	visible = not player.is_empty()
	if player.is_empty():
		return
	var label := "%s  %d PTS  %d/%d" % [
		str(player.get("name", "P")),
		int(player.get("score", 0)),
		int(player.get("ammo", 0)),
		int(player.get("max_ammo", 6)),
	]
	if bool(player.get("reloading", false)):
		label = "%s RELOAD %.1f" % [label, float(player.get("reload_left", 0.0))]
	info_label.text = label
	info_label.modulate = color
	var hp := int(player.get("hp", 0))
	for index in range(hp_container.get_child_count()):
		var block := hp_container.get_child(index) as ColorRect
		block.color = color if index < hp else Color("#28313d")
	var ratio := 1.0 - clampf(
		float(player.get("dash_cooldown_left", 0.0)) / maxf(dash_cooldown, 0.01),
		0.0,
		1.0
	)
	dash_bar.value = ratio * 100.0
	dash_bar.modulate = color
