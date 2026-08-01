extends CanvasLayer

const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")
const YELLOW := Color("#ffe66d")
const GREEN := Color("#7cff6b")

## ID順に割り当てる色。下部ステータスと表彰台で同じ色になるよう1箇所にまとめる。
const PLAYER_COLORS := [CYAN, MAGENTA, YELLOW, GREEN]

@onready var player_one_status = %PlayerOneStatus
@onready var player_two_status = %PlayerTwoStatus
@onready var player_three_status = %PlayerThreeStatus
@onready var player_four_status = %PlayerFourStatus
@onready var match_label: Label = %MatchLabel
@onready var connection_label: Label = %ConnectionLabel
@onready var countdown_label: Label = %CountdownLabel
@onready var no_ammo_label: Label = %NoAmmoLabel
@onready var result_overlay: Control = %ResultOverlay
@onready var result_label: Label = %ResultLabel
@onready var result_podium: ResultPodium = %ResultPodium

var no_ammo_tween: Tween


func set_connection_status(text: String) -> void:
	connection_label.text = text


func show_no_ammo() -> void:
	clear_no_ammo()
	no_ammo_label.visible = true
	no_ammo_label.modulate = Color.WHITE
	no_ammo_label.scale = Vector2(1.12, 1.12)
	no_ammo_tween = create_tween()
	no_ammo_tween.tween_property(no_ammo_label, "scale", Vector2.ONE, 0.08).set_trans(
		Tween.TRANS_BACK
	).set_ease(Tween.EASE_OUT)
	no_ammo_tween.tween_interval(0.45)
	no_ammo_tween.tween_property(no_ammo_label, "modulate:a", 0.0, 0.2)
	no_ammo_tween.tween_callback(func(): no_ammo_label.visible = false)


func clear_no_ammo() -> void:
	if no_ammo_tween != null and no_ammo_tween.is_valid():
		no_ammo_tween.kill()
	no_ammo_tween = null
	if is_instance_valid(no_ammo_label):
		no_ammo_label.visible = false


func apply_snapshot(
	players: Array,
	phase: String,
	time_left: float,
	winner_id,
	reconnect_grace_left: float,
	dash_cooldown: float
) -> void:
	var sorted := players.duplicate()
	sorted.sort_custom(func(a, b): return int(a.get("id", 0)) < int(b.get("id", 0)))
	var colors := {}
	for index in range(sorted.size()):
		colors[int(sorted[index].get("id", 0))] = PLAYER_COLORS[index % PLAYER_COLORS.size()]
	player_one_status.apply_player(sorted[0] if sorted.size() > 0 else {}, CYAN, dash_cooldown)
	player_two_status.apply_player(sorted[1] if sorted.size() > 1 else {}, MAGENTA, dash_cooldown)
	player_three_status.apply_player(sorted[2] if sorted.size() > 2 else {}, YELLOW, dash_cooldown)
	player_four_status.apply_player(sorted[3] if sorted.size() > 3 else {}, GREEN, dash_cooldown)

	var center_text := _format_time(time_left)
	if phase == "waiting":
		center_text = "WAITING IN LOBBY"
	elif phase == "countdown":
		center_text = "MATCH START  %d" % int(ceil(time_left))
	elif phase == "paused":
		center_text = "RECONNECTING... %.1f" % reconnect_grace_left
	elif phase == "match_finished":
		center_text = "DRAW"
		if winner_id != null:
			var winner_name := "PLAYER %d" % int(winner_id)
			var winner_score := 0
			for player in players:
				if int(player.get("id", 0)) == int(winner_id):
					winner_name = str(player.get("name", winner_name))
					winner_score = int(player.get("score", 0))
					break
			center_text = "%s WINS  %d PTS" % [winner_name, winner_score]
	match_label.text = center_text
	countdown_label.visible = phase == "countdown"
	countdown_label.text = str(int(ceil(time_left)))
	result_overlay.visible = phase == "match_finished"
	# 結果画面では同じ文をResultLabelが大きく出すので、上部の表示は隠す。
	match_label.visible = not result_overlay.visible
	if result_overlay.visible:
		result_podium.apply(players, colors)
	result_label.text = center_text


func _format_time(time_left: float) -> String:
	var total_seconds := maxi(int(ceil(time_left)), 0)
	return "%02d:%02d" % [total_seconds / 60, total_seconds % 60]
