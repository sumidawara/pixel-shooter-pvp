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
	round_number: int,
	round_winner_id,
	winner_id,
	reconnect_grace_left: float,
	dash_cooldown: float
) -> void:
	var sorted := players.duplicate()
	sorted.sort_custom(func(a, b): return int(a.get("id", 0)) < int(b.get("id", 0)))
	player_one_status.apply_player(sorted[0] if sorted.size() > 0 else {}, CYAN, dash_cooldown)
	player_two_status.apply_player(sorted[1] if sorted.size() > 1 else {}, MAGENTA, dash_cooldown)

	var center_text := "%02d" % int(ceil(time_left))
	if phase == "waiting":
		center_text = "WAITING FOR 2 PLAYERS"
	elif phase == "countdown":
		center_text = "ROUND %d  %d" % [round_number, int(ceil(time_left))]
	elif phase == "overtime":
		center_text = "OVERTIME  %02d" % int(ceil(time_left))
	elif phase == "round_end":
		center_text = "ROUND WINNER"
		if round_winner_id != null:
			center_text = "PLAYER %d TAKES ROUND" % int(round_winner_id)
	elif phase == "paused":
		center_text = "RECONNECTING... %.1f" % reconnect_grace_left
	elif phase == "match_finished":
		center_text = "DRAW"
		if winner_id != null:
			center_text = "PLAYER %d WINS MATCH" % int(winner_id)
	match_label.text = center_text
	countdown_label.visible = phase == "countdown"
	countdown_label.text = str(int(ceil(time_left)))
	result_overlay.visible = phase == "match_finished"
	result_label.text = center_text
