extends SceneTree

## ルーム設定が、サーバーの持ち物として扱われているかの検証。
##
## ルームの設定を決めるのはサーバー（server.json）で、クライアントは編集して
## 送り返すだけ。この向きが崩れると、設定ファイルに書いた値が黙って効かなくなる。
## 実際、部屋を作った瞬間にクライアントの初期値で上書きされていた。
##
##     godot --headless --path frontend --script res://tests/room_settings_test.gd

## サーバーが持っているが、画面には出していない設定。
##
## こういう項目こそ、クライアントが勝手に値を作ると事故になる。
const SERVER_ONLY_KEY := "item_spawn_interval"

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_nothing_is_sent_before_the_server_speaks()
	await _check_settings_the_screen_does_not_show_are_echoed_back()
	await _check_every_editable_setting_makes_the_round_trip()
	await _check_creating_a_room_sends_no_settings()

	if not _failures.is_empty():
		push_error("room settings:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("room settings: サーバーの設定を上書きしないことを確認した")
	quit(0)


## サーバーから設定が届く前は、何も送らないこと。
##
## 届く前の画面の値はシーンに書かれた初期値でしかない。それを送ると、
## サーバーが server.json から決めた設定を潰す。
func _check_nothing_is_sent_before_the_server_speaks() -> void:
	var menu = await _open_menu()
	menu.is_room_host = true
	var sent: Array = []
	menu.room_settings_changed.connect(func(settings: Dictionary): sent.append(settings))

	menu.max_items_input.value = 9
	menu.sandbox_check.button_pressed = true
	await process_frame
	if not sent.is_empty():
		_failures.append("サーバーから届く前に送っている: %s" % [sent])

	# 一度届いたら、以降の編集は送る。
	menu._apply_room_settings(_server_settings())
	menu.max_items_input.value = 7
	await process_frame
	if sent.is_empty():
		_failures.append("届いた後も送っていない。設定を変えられない")

	await _close(menu)


## 画面に無い設定を、クライアントが作らずそのまま返すこと。
func _check_settings_the_screen_does_not_show_are_echoed_back() -> void:
	var menu = await _open_menu()
	var from_server := _server_settings()
	from_server[SERVER_ONLY_KEY] = 12.5

	menu._apply_room_settings(from_server)
	var returned: Dictionary = menu.get_room_settings()

	if not returned.has(SERVER_ONLY_KEY):
		_failures.append("%s が返らない。サーバー側で既定値へ落ちる" % SERVER_ONLY_KEY)
	elif not is_equal_approx(float(returned[SERVER_ONLY_KEY]), 12.5):
		_failures.append(
			"%s をクライアントが作り替えている: %s" % [SERVER_ONLY_KEY, returned[SERVER_ONLY_KEY]]
		)

	await _close(menu)


## 画面で編集できる設定が、届いて・戻って・型を保つこと。
##
## 整数の項目を小数で送るとサーバーが受け取れない。
func _check_every_editable_setting_makes_the_round_trip() -> void:
	var menu = await _open_menu()
	var from_server := _server_settings()
	from_server["match_seconds"] = 240.0
	from_server["kill_points"] = 55
	from_server["death_penalty"] = 15
	from_server["item_points"] = 35
	from_server["max_items"] = 9
	from_server["cpu_level"] = 1
	from_server["sandbox"] = true

	menu._apply_room_settings(from_server)
	var returned: Dictionary = menu.get_room_settings()

	for key in from_server:
		if not returned.has(key):
			_failures.append("%s が返らない" % key)
			continue
		if str(returned[key]) != str(from_server[key]):
			_failures.append(
				"%s が往復で変わった: %s -> %s" % [key, from_server[key], returned[key]]
			)
	for key in ["kill_points", "death_penalty", "item_points", "max_items", "cpu_level"]:
		if typeof(returned.get(key)) != TYPE_INT:
			_failures.append("%s が整数で返らない。サーバーが受け取れない" % key)

	# 編集した値が送る側にも乗ること。
	menu.max_items_input.value = 4
	if int(menu.get_room_settings().get("max_items", 0)) != 4:
		_failures.append("画面で変えた値が送られない")

	await _close(menu)


## 部屋を作るとき、設定を渡さないこと。
func _check_creating_a_room_sends_no_settings() -> void:
	var menu = await _open_menu()
	var arguments: Array = []
	menu.create_requested.connect(
		func(name: String, port: int): arguments.append([name, port])
	)

	menu._request_create_room()
	await process_frame

	if arguments.is_empty():
		_failures.append("CREATE ROOM が伝わらない")
	elif arguments[0].size() != 2:
		_failures.append("作成時に設定を渡している: %s" % [arguments[0]])

	await _close(menu)


## サーバーから届く設定の形。
func _server_settings() -> Dictionary:
	return {
		"map_id": "classic_arena",
		"match_seconds": 120.0,
		"kill_points": 100,
		"death_penalty": 25,
		"item_points": 20,
		"item_spawn_interval": 5.0,
		"max_items": 3,
		"sandbox": false,
		"cpu_level": 3,
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
