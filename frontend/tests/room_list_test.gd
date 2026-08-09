extends SceneTree

## ルーム一覧画面の検証。
##
## 一覧は必ず古い。GameServerの報告間隔ぶん遅れるので「押した瞬間に満室」は
## 普通に起こる。断られたときに一覧へ戻らないと、選び直すのにタイトルから
## やり直すことになる。
##
## 接続先を変えるモーダルは一覧の上に重ねる。一覧を消すと、今どこを見ているのか
## 分からないまま新しいURLを入れることになる。
##
##     godot --headless --path frontend --script res://tests/room_list_test.gd

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_open_rooms_can_be_entered()
	await _check_a_full_room_cannot_be_pressed()
	await _check_an_empty_lobby_says_so()
	await _check_the_modal_sits_on_top_of_the_list()
	await _check_changing_the_server_refetches()
	await _check_a_refused_join_returns_to_the_list()
	_check_the_directory_normalises_addresses()

	if not _failures.is_empty():
		push_error("room list:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("room list: 一覧・モーダル・断られたときの戻り先が期待どおりだった")
	quit(0)


## 入れる部屋を押すと、その部屋へ繋ぎに行くこと。
func _check_open_rooms_can_be_entered() -> void:
	var menu = await _open_menu()
	var requested: Array = []
	menu.join_requested.connect(func(url: String, _name: String): requested.append(url))

	menu.show_rooms([_room("HOST-A", 2, true, "ws://10.0.0.2:9001")])
	await process_frame

	var buttons: Array = menu.room_list_box.get_children()
	if buttons.is_empty():
		_failures.append("一覧に行が出ない")
		await _close(menu)
		return
	buttons[0].pressed.emit()
	if requested != ["ws://10.0.0.2:9001"]:
		_failures.append("押した部屋へ繋ぎに行かない: %s" % [requested])

	await _close(menu)


## 満室・試合中の部屋は押せないこと。
##
## 押してから断られるより、押せないほうが早く分かる。
func _check_a_full_room_cannot_be_pressed() -> void:
	var menu = await _open_menu()
	menu.show_rooms([
		_room("OPEN", 1, true, "ws://a:9001"),
		_room("BUSY", 4, false, "ws://b:9001"),
	])
	await process_frame

	var buttons: Array = menu.room_list_box.get_children()
	if buttons.size() != 2:
		_failures.append("行の数が合わない: %d" % buttons.size())
		await _close(menu)
		return
	if buttons[0].disabled:
		_failures.append("入れる部屋が押せない")
	if not buttons[1].disabled:
		_failures.append("満室の部屋が押せてしまう")

	await _close(menu)


## 部屋が無いときに、そう言うこと。
##
## 空欄のままだと「部屋が無い」と「繋がらない」が区別できない。
func _check_an_empty_lobby_says_so() -> void:
	var menu = await _open_menu()
	menu.show_rooms([])
	await process_frame
	if menu.room_list_status.text.strip_edges().is_empty():
		_failures.append("部屋が無いときに何も言わない")
	if not menu.room_list_box.get_children().is_empty():
		_failures.append("部屋が無いのに行が残っている")
	await _close(menu)


## 接続先のモーダルが、一覧を消さずに重なること。
func _check_the_modal_sits_on_top_of_the_list() -> void:
	var menu = await _open_menu()
	menu.show_room_list()
	await process_frame

	menu.change_server_button.pressed.emit()
	await process_frame
	if not menu.join_page.visible:
		_failures.append("接続先のモーダルが開かない")
	if not menu.room_list_page.visible:
		_failures.append("モーダルを開くと一覧が消える")
	if menu.join_page.get_index() < menu.room_list_page.get_index():
		_failures.append("モーダルが一覧の下に描かれる")

	menu.join_page.get_node("%JoinBackButton").pressed.emit()
	await process_frame
	if menu.join_page.visible:
		_failures.append("モーダルを閉じられない")

	await _close(menu)


## URLを入れて検索すると、モーダルが消えてその接続先を引き直すこと。
func _check_changing_the_server_refetches() -> void:
	var menu = await _open_menu()
	var asked: Array = []
	menu.lobby_url_changed.connect(func(url: String): asked.append(url))

	menu.show_room_list()
	await process_frame
	menu.change_server_button.pressed.emit()
	menu.server_input.text = "http://10.0.0.9:8080"
	menu.join_button.pressed.emit()
	await process_frame

	if menu.join_page.visible:
		_failures.append("検索してもモーダルが消えない")
	if menu.lobby_url != "http://10.0.0.9:8080":
		_failures.append("接続先が変わらない: %s" % menu.lobby_url)
	if not asked.has("http://10.0.0.9:8080"):
		_failures.append("新しい接続先で引き直さない: %s" % [asked])
	if not menu.lobby_label.text.contains("10.0.0.9"):
		_failures.append("画面に今の接続先が出ていない: %s" % menu.lobby_label.text)

	await _close(menu)


## 入れなかったときに、一覧へ戻ること。
func _check_a_refused_join_returns_to_the_list() -> void:
	var menu = await _open_menu()
	menu.show_room_failed("ROOM IS FULL")
	await process_frame
	if not menu.room_list_page.visible:
		_failures.append("断られるとタイトルまで戻ってしまう")
	await _close(menu)


## ws:// を渡されても一覧を引けること。
##
## 画面には接続先が1つしか出ないので、人は ws:// と http:// を区別しない。
func _check_the_directory_normalises_addresses() -> void:
	var directory := RoomDirectory.new()
	var cases := {
		"ws://127.0.0.1:8080": "http://127.0.0.1:8080",
		"wss://example.test": "https://example.test",
		"http://example.test": "http://example.test",
		"example.test:8080": "http://example.test:8080",
		"": "",
	}
	for input in cases:
		var actual: String = directory._as_http_url(input)
		if actual != cases[input]:
			_failures.append("%s -> %s（期待 %s）" % [input, actual, cases[input]])
	directory.free()


func _room(host: String, players: int, open: bool, url: String) -> Dictionary:
	return {
		"game_url": url,
		"host_name": host,
		"player_count": players,
		"max_players": 4,
		"accepting_players": open,
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
