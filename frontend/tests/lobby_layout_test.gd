extends SceneTree

## ロビーの作りの検証。
##
## 設定を「種類」でまとめると雑多な箱ができる。以前はCPUの強さが試合ルールの欄に
## 並んでいて、「この試合の決まり」と「この1体の性質」が同じ大きさで混ざっていた。
## 設定は、それが属する対象の行に置く。
##
## 併せて、色が参加者の並び順から決まっていた問題も見る。誰かが抜けると残った
## 全員の色がずれるため、試合中に見分けの手がかりが入れ替わっていた。
##
##     godot --headless --path frontend --script res://tests/lobby_layout_test.gd

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_the_title_reaches_both_ways_in_one_step()
	await _check_the_scoring_numbers_start_folded()
	await _check_cpu_strength_lives_on_the_cpu_row()
	await _check_you_can_pick_your_own_colour()
	await _check_a_colour_someone_else_holds_is_not_offered()
	await _check_colours_survive_someone_leaving()

	if not _failures.is_empty():
		push_error("lobby layout:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("lobby layout: 設定の置き場所と色の決まり方が期待どおりだった")
	quit(0)


## タイトルから、作るのも入るのも1手で届くこと。
##
## 間に二択だけの画面を挟むと、まだ何も決めていない人に決めさせることになる。
func _check_the_title_reaches_both_ways_in_one_step() -> void:
	var menu = await _open_menu()

	if menu.has_node("PlayPage"):
		_failures.append("PlayPage が残っている")
	for name in ["PlayButton", "TitleJoinButton"]:
		if not menu.title_page.has_node("Actions/" + name):
			_failures.append("タイトルに %s が無い" % name)

	menu.title_join_button.pressed.emit()
	await process_frame
	if not menu.room_list_page.visible:
		_failures.append("JOIN からルーム一覧へ行けない")

	await _close(menu)


## 点数まわりが、最初は畳まれていること。
##
## 試合を始めるのに要る判断は「誰と」「どこで」であって、撃破点が何点かではない。
func _check_the_scoring_numbers_start_folded() -> void:
	var menu = await _open_menu()

	if menu.advanced_box.visible:
		_failures.append("点数の詳細が最初から開いている")
	for name in ["MatchSecondsInput", "KillPointsInput", "DeathPenaltyInput"]:
		if not menu.advanced_box.has_node(name) and not menu.advanced_box.has_node("ItemRow/" + name):
			_failures.append("%s が詳細の中に入っていない" % name)

	menu.advanced_toggle.pressed.emit()
	await process_frame
	if not menu.advanced_box.visible:
		_failures.append("詳細を開けない")

	await _close(menu)


## CPUの強さが、そのCPUの行にあること。
func _check_cpu_strength_lives_on_the_cpu_row() -> void:
	var menu = await _open_menu()
	menu.is_room_host = true
	var changes: Array = []
	menu.cpu_level_changed.connect(func(id: int, level: int): changes.append([id, level]))

	menu.apply_room_snapshot(
		[_player(1, 0), _cpu(2, 1, 2)], _room(1), 1
	)
	await process_frame

	var picker := _find_option(_row_for(menu, "CPU-2"))
	if picker == null:
		_failures.append("CPUの行に強さの選択が無い")
		await _close(menu)
		return
	if picker.selected != 1:
		_failures.append("届いた強さが行に出ていない: %d" % (picker.selected + 1))

	picker.item_selected.emit(3)
	if changes.is_empty():
		_failures.append("強さを変えても送られない")
	elif changes[0] != [2, 4]:
		_failures.append("送る中身が違う: %s" % [changes[0]])

	# 人間の行には強さを出さない。
	#
	# 選択肢の数では見分けられない。色も段階もどちらも4つある。
	var human_picker := _find_option(_row_for(menu, "P1"))
	if human_picker != null and human_picker.get_item_text(0) == menu.CPU_LEVEL_LABELS[0]:
		_failures.append("人間の行にCPUの強さが出ている")

	await _close(menu)


## 自分の行から色を選べること。
func _check_you_can_pick_your_own_colour() -> void:
	var menu = await _open_menu()
	var chosen: Array = []
	menu.color_chosen.connect(func(color: int): chosen.append(color))

	menu.apply_room_snapshot([_player(1, 0)], _room(1), 1)
	await process_frame

	var picker := _find_option(_row_for(menu, "P1"))
	if picker == null:
		_failures.append("自分の行に色の選択が無い")
		await _close(menu)
		return
	picker.item_selected.emit(2)
	if chosen != [2]:
		_failures.append("色を選んでも送られない: %s" % [chosen])

	await _close(menu)


## 他の人が持っている色は選べないこと。
##
## 押せてしまうとサーバーに無視されるだけで、なぜ変わらないのか分からない。
func _check_a_colour_someone_else_holds_is_not_offered() -> void:
	var menu = await _open_menu()
	menu.apply_room_snapshot([_player(1, 0), _player(2, 1)], _room(1), 1)
	await process_frame

	var picker := _find_option(_row_for(menu, "P1"))
	if picker == null:
		_failures.append("自分の行に色の選択が無い")
		await _close(menu)
		return
	if not picker.is_item_disabled(1):
		_failures.append("他の人が使っている色が選べてしまう")
	if picker.is_item_disabled(0):
		_failures.append("今の自分の色が選べなくなっている")
	if picker.is_item_disabled(2):
		_failures.append("空いている色が選べない")

	await _close(menu)


## 誰かが抜けても、残った人の色が変わらないこと。
##
## 以前は並び順で決めていたため、先頭の人が抜けると全員の色が1つずつずれた。
func _check_colours_survive_someone_leaving() -> void:
	var menu = await _open_menu()

	menu.apply_room_snapshot([_player(1, 0), _player(2, 1), _player(3, 2)], _room(1), 2)
	await process_frame
	var before := _swatch_colour(_row_for(menu, "P2"))

	# 先頭が抜ける。残った2人の色番号は動かない。
	menu.apply_room_snapshot([_player(2, 1), _player(3, 2)], _room(2), 2)
	await process_frame
	var after := _swatch_colour(_row_for(menu, "P2"))

	if before != after:
		_failures.append("誰かが抜けると自分の色が変わる: %s -> %s" % [before, after])

	await _close(menu)


func _row_for(menu, name: String) -> Control:
	for row in menu.room_players.get_children():
		for child in row.get_children() if row is HBoxContainer else []:
			if child is Label and child.text == name:
				return row
	return null


func _find_option(row: Control) -> OptionButton:
	if row == null:
		return null
	for child in row.get_children():
		if child is OptionButton:
			return child
	return null


func _swatch_colour(row: Control) -> Color:
	if row == null:
		return Color.BLACK
	for child in row.get_children():
		if child is ColorRect:
			return child.color
	return Color.BLACK


func _player(id: int, color: int) -> Dictionary:
	return {
		"id": id,
		"name": "P%d" % id,
		"is_cpu": false,
		"is_dummy": false,
		"cpu_level": 0,
		"color": color,
		"connected": true,
	}


func _cpu(id: int, color: int, level: int) -> Dictionary:
	return {
		"id": id,
		"name": "CPU-%d" % id,
		"is_cpu": true,
		"is_dummy": false,
		"cpu_level": level,
		"color": color,
		"connected": true,
	}


func _room(host_id: int) -> Dictionary:
	return {
		"host_player_id": host_id,
		"can_start": true,
		"max_players": 4,
		"settings": {"map_id": "classic_arena", "sandbox": false},
	}


func _open_menu():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main.get_node("MenuScreen")


func _close(menu) -> void:
	menu.get_parent().queue_free()
	await process_frame
