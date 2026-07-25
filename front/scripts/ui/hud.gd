extends CanvasLayer

const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")

@onready var player_one_status = %PlayerOneStatus
@onready var player_two_status = %PlayerTwoStatus
@onready var match_label: Label = %MatchLabel
@onready var connection_label: Label = %ConnectionLabel
@onready var countdown_label: Label = %CountdownLabel
@onready var result_overlay: Control = %ResultOverlay
@onready var result_label: Label = %ResultLabel


func set_connection_status(text: String) -> void:
	connection_label.text = text


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
	player_one_status.apply_player(sorted[0] if sorted.size() > 0 else {}, CYAN, dash_cooldown)
	player_two_status.apply_player(sorted[1] if sorted.size() > 1 else {}, MAGENTA, dash_cooldown)

	var center_text := _format_time(time_left)
	if phase == "waiting":
		center_text = "WAITING FOR 2 PLAYERS"
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
	result_label.text = center_text


func _format_time(time_left: float) -> String:
	var total_seconds := maxi(int(ceil(time_left)), 0)
	return "%02d:%02d" % [total_seconds / 60, total_seconds % 60]
