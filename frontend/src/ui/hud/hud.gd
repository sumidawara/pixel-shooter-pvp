extends CanvasLayer

const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")
const YELLOW := Color("#ffe66d")
const GREEN := Color("#7cff6b")

## ID順に割り当てる色。HUD、レーダー、表彰台で同じ色になるよう1箇所にまとめる。
const PLAYER_COLORS := [CYAN, MAGENTA, YELLOW, GREEN]

@onready var player_one_status = %PlayerOneStatus
@onready var player_two_status = %PlayerTwoStatus
@onready var player_three_status = %PlayerThreeStatus
@onready var player_four_status = %PlayerFourStatus
@onready var match_label: Label = %MatchLabel
@onready var connection_label: Label = %ConnectionLabel
@onready var countdown_label: Label = %CountdownLabel
@onready var result_overlay: Control = %ResultOverlay
@onready var result_label: Label = %ResultLabel
@onready var result_podium: ResultPodium = %ResultPodium
@onready var item_slot = %ItemSlot
@onready var radar_display = %RadarDisplay

func set_connection_status(text: String) -> void:
	connection_label.text = "// LINK  " + text


func apply_snapshot(
	players: Array,
	local_player_id: int,
	phase: String,
	time_left: float,
	winner_id,
	reconnect_grace_left: float,
	dash_cooldown: float,
	local_player: Dictionary
) -> void:
	item_slot.apply_player(local_player)
	var sorted_by_id := players.duplicate()
	sorted_by_id.sort_custom(func(a, b): return int(a.get("id", 0)) < int(b.get("id", 0)))
	var colors := {}
	for index in range(sorted_by_id.size()):
		colors[int(sorted_by_id[index].get("id", 0))] = PLAYER_COLORS[index % PLAYER_COLORS.size()]

	radar_display.apply_snapshot(players, local_player_id, colors)
	var ranked := players.duplicate()
	ranked.sort_custom(func(a, b):
		var score_a := int(a.get("score", 0))
		var score_b := int(b.get("score", 0))
		return score_a > score_b if score_a != score_b else int(a.get("id", 0)) < int(b.get("id", 0))
	)
	var ranks := {}
	for index in range(ranked.size()):
		ranks[int(ranked[index].get("id", 0))] = index + 1

	# カード位置が順位更新のたびに動かないよう、自機を先頭、他プレイヤーをID順に固定する。
	var display_players: Array = []
	for player in sorted_by_id:
		if int(player.get("id", 0)) == local_player_id:
			display_players.append(player)
			break
	for player in sorted_by_id:
		if int(player.get("id", 0)) != local_player_id:
			display_players.append(player)

	var status_views := [
		player_one_status,
		player_two_status,
		player_three_status,
		player_four_status,
	]
	for index in range(status_views.size()):
		var player: Dictionary = display_players[index] if index < display_players.size() else {}
		var id := int(player.get("id", 0))
		status_views[index].apply_player(
			player,
			colors.get(id, PLAYER_COLORS[index]),
			dash_cooldown,
			int(ranks.get(id, index + 1)),
			index == 0 and id == local_player_id
		)

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
