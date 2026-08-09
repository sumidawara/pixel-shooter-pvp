extends SceneTree

## 練習場（サンドボックス）が、画面の上で正しく伝わるかの検証。
##
## サーバー側の挙動は scripts/sandbox_test.mjs が通しで見ている。ここで見るのは
## 「設定がサーバーへ届くか」と「今どういう場にいるかが画面から分かるか」。
## 撃ち返してこない相手や終わらない試合は、知らないと不具合に見える。
##
##     godot --headless --path frontend --script res://tests/sandbox_ui_test.gd

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_the_toggle_reaches_the_room_settings()
	await _check_the_start_button_says_what_will_happen()
	await _check_the_hud_shows_the_sandbox_instead_of_a_frozen_clock()
	_check_dummies_are_marked_apart_from_cpus()

	if not _failures.is_empty():
		push_error("sandbox ui:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("sandbox ui: 設定の往復と画面表示が期待どおりだった")
	quit(0)


## 設定がサーバーへ送る辞書に入り、返ってきたSnapshotから復元できること。
##
## 片道でも欠けると、ホストの画面と実際のルームの状態が食い違う。
func _check_the_toggle_reaches_the_room_settings() -> void:
	var menu = await _open_menu()

	menu.sandbox_check.button_pressed = true
	var settings: Dictionary = menu.get_room_settings()
	if not bool(settings.get("sandbox", false)):
		_failures.append("トグルがルーム設定に入らない。サーバーへ届かない")

	# 別の人がホストの場合、設定はSnapshot経由で降ってくる。
	menu.sandbox_check.button_pressed = false
	menu._apply_room_settings({"sandbox": true, "map_id": "classic_arena"})
	if not menu.sandbox_check.button_pressed:
		_failures.append("Snapshotの設定が画面へ戻らない。今どちらか分からない")

	await _close(menu)


## 押す前に何が起きるか分かること。練習場では相手のCPUは足されない。
func _check_the_start_button_says_what_will_happen() -> void:
	var menu = await _open_menu()
	menu.is_room_host = true

	menu.sandbox_check.button_pressed = false
	menu._update_host_controls(1, true)
	var versus_text: String = menu.start_button.text

	menu.sandbox_check.button_pressed = true
	menu._update_host_controls(1, true)
	var sandbox_text: String = menu.start_button.text

	if sandbox_text == versus_text:
		_failures.append("練習場でも対戦と同じ表示のまま: %s" % sandbox_text)
	elif not sandbox_text.contains("SANDBOX"):
		_failures.append("練習場が始まると読み取れない: %s" % sandbox_text)
	if versus_text.contains("SANDBOX"):
		_failures.append("対戦なのに練習場と表示している: %s" % versus_text)

	await _close(menu)


## 進まない時計ではなく、練習場であることを出すこと。
##
## 練習場は時間で終わらないため、残り時間の表示は動かない。止まった時計は
## 「壊れている」としか読めず、何も伝えない。
func _check_the_hud_shows_the_sandbox_instead_of_a_frozen_clock() -> void:
	var hud_scene: PackedScene = load("res://src/ui/hud/hud.tscn")
	var hud = hud_scene.instantiate()
	root.add_child(hud)
	await process_frame

	var players := [{"id": 1, "name": "P1", "hp": 5, "max_hp": 5, "score": 0}]
	hud.apply_snapshot(players, 1, "running", 118.0, null, 0.0, 1.1, players[0], false)
	var versus_text: String = hud.match_label.text
	hud.apply_snapshot(players, 1, "running", 118.0, null, 0.0, 1.1, players[0], true)
	var sandbox_text: String = hud.match_label.text

	if not versus_text.contains("58") and not versus_text.contains("01:58"):
		_failures.append("対戦で残り時間が出ていない: %s" % versus_text)
	if sandbox_text == versus_text:
		_failures.append("練習場でも進まない時計を出している: %s" % sandbox_text)
	elif not sandbox_text.contains("SANDBOX"):
		_failures.append("練習場だと分からない: %s" % sandbox_text)

	hud.queue_free()
	await process_frame


## 撃ち返してくるCPUと、撃ち返してこない的を見分けられること。
##
## どちらも is_cpu なので、印が同じだと画面の上で区別がつかない。
func _check_dummies_are_marked_apart_from_cpus() -> void:
	var status_scene: PackedScene = load("res://src/ui/hud/player_status.tscn")
	var status = status_scene.instantiate()
	root.add_child(status)

	var base := {"hp": 5, "max_hp": 5, "score": 0, "ammo": 6, "max_ammo": 6}
	var labels: Array[String] = []
	for player in [
		{"name": "HUMAN", "is_cpu": false, "is_dummy": false},
		{"name": "CPU", "is_cpu": true, "is_dummy": false},
		{"name": "DUMMY", "is_cpu": true, "is_dummy": true},
	]:
		status.apply_player(base.merged(player), Color.WHITE, 1.1, 1, false)
		labels.append(status.name_label.text)

	if labels[0] == labels[1] or labels[1] == labels[2] or labels[0] == labels[2]:
		_failures.append("人・CPU・的の印が重なっている: %s" % [labels])

	status.queue_free()


func _open_menu():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main.get_node("MenuScreen")


func _close(menu) -> void:
	menu.get_parent().queue_free()
	await process_frame
