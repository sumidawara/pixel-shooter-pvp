extends SceneTree

## CREATE ROOM でサーバーをホストするときの、失敗経路と監視の検証。
##
## 成功経路は room_flow_test が通しで見ている。ここで見るのは、
## 起動できなかった／落ちたときに、原因が分かる形で伝わるか。
## この経路は普段動かないので、壊れても気付けない。
##
##     godot --headless --path frontend --script res://tests/host_server_test.gd

const HostServerController := preload("res://src/networking/host_server_controller.gd")

## 他の試験やCompose環境と重ならない範囲を使う。
const GAME_PORT := 9061

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_control_port_collision_is_avoided()
	await _check_server_starts_and_writes_a_log()
	await _check_death_is_noticed()
	_check_missing_binary_message_lists_paths()

	await _check_busy_port_falls_back_to_a_free_one()
	await _check_failed_hosting_leaves_the_room_screen()

	if not _failures.is_empty():
		push_error("host server:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("host server: 失敗経路と生存監視が期待どおりだった")
	quit(0)


## 希望のポートが埋まっていても、空いている番号で開けること。
##
## 配布版を遊ぶ人にとってポート番号は本来どうでもよく、埋まっているだけで
## 部屋を作れないのは行き止まりになる。
func _check_busy_port_falls_back_to_a_free_one() -> void:
	# 希望のポートと、その制御APIポートの両方を塞ぐ。
	var blockers: Array[TCPServer] = []
	for blocked in [GAME_PORT, GAME_PORT + HostServerController.CONTROL_PORT_OFFSET]:
		var blocker := TCPServer.new()
		if blocker.listen(blocked, NetworkConfig.LOCAL_SERVER_HOST) != OK:
			_failures.append("検査用にポート%dを確保できなかった" % blocked)
			return
		blockers.append(blocker)

	var controller = HostServerController.new()
	root.add_child(controller)
	var started: Array[String] = []
	var moved: Array[int] = []
	controller.server_started.connect(func(url: String) -> void: started.append(url))
	controller.port_changed.connect(
		func(_requested: int, used: int) -> void: moved.append(used)
	)
	controller.server_failed.connect(
		func(text: String) -> void: _failures.append("空きがあるのに失敗した: %s" % text)
	)

	controller.start_server(GAME_PORT)
	await process_frame

	if started.is_empty():
		_failures.append("希望のポートが埋まっていると起動できない")
	elif started[0].ends_with(":%d" % GAME_PORT):
		_failures.append("塞いだはずのポートで起動している: %s" % started[0])
	if moved.is_empty():
		_failures.append("ポートを変えたことが伝わらない。他の人が接続先を知れない")
	elif not started.is_empty() and not started[0].contains(str(moved[0])):
		_failures.append("通知した番号と実際の接続先が違う: %s / %d" % [started[0], moved[0]])

	controller.stop_server()
	controller.queue_free()
	for blocker in blockers:
		blocker.stop()
	await process_frame


## 起動できなかったとき、操作できないルーム画面に取り残されないこと。
##
## ルーム画面は起動を待たずに開くため、失敗しても閉じないと
## ADD CPU も START GAME も効かない画面で詰む。
func _check_failed_hosting_leaves_the_room_screen() -> void:
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame

	var menu = main.get_node("MenuScreen")
	# CREATE ROOM と同じ順序で、先にルーム画面を開いてから失敗させる。
	menu.show_room(true, "ws://127.0.0.1:9001")
	if not menu.create_page.visible:
		_failures.append("検査の前提が崩れている: ルーム画面が開かない")
	main._on_local_server_failed("PORT 9001 IS ALREADY IN USE")
	await process_frame

	if menu.create_page.visible:
		_failures.append("起動に失敗してもルーム画面に留まっている")
	if not menu.status_label.text.contains("9001"):
		_failures.append("理由が表示されていない: %s" % menu.status_label.text)

	main.queue_free()
	await process_frame


## 制御APIのポートだけが埋まっている場合も、そこでは開かないこと。
##
## 以前は --debug-bind を渡しておらず制御APIは常に9101だった。衝突しても
## サーバーは起動を続けるため、管理機能だけが黙って死んでいた。
## 今はゲーム用と制御API用が両方空いている組を選ぶ。
func _check_control_port_collision_is_avoided() -> void:
	var control_port := GAME_PORT + HostServerController.CONTROL_PORT_OFFSET
	var blocker := TCPServer.new()
	if blocker.listen(control_port, NetworkConfig.LOCAL_SERVER_HOST) != OK:
		_failures.append("検査用にポート%dを確保できなかった" % control_port)
		return

	var controller = HostServerController.new()
	root.add_child(controller)
	var started: Array[String] = []
	controller.server_started.connect(func(url: String) -> void: started.append(url))
	controller.server_failed.connect(
		func(text: String) -> void: _failures.append("空きがあるのに失敗した: %s" % text)
	)
	controller.start_server(GAME_PORT)
	await process_frame

	if started.is_empty():
		_failures.append("制御APIのポートが埋まっているだけで起動できない")
	elif started[0].ends_with(":%d" % GAME_PORT):
		_failures.append(
			"制御APIが埋まっているのにその組で起動した: %s" % started[0]
		)

	blocker.stop()
	controller.stop_server()
	controller.queue_free()
	await process_frame


## 起動できること、そしてログがファイルへ残ること。
func _check_server_starts_and_writes_a_log() -> void:
	var controller = HostServerController.new()
	root.add_child(controller)
	var started: Array[String] = []
	controller.server_started.connect(func(url: String) -> void: started.append(url))
	controller.server_failed.connect(
		func(text: String) -> void: _failures.append("起動できなかった: %s" % text)
	)

	# 前回の内容と混ざらないよう消してから始める。
	var log_path := ProjectSettings.globalize_path("user://server.log")
	DirAccess.remove_absolute(log_path)

	controller.start_server(GAME_PORT)
	await process_frame
	if started.is_empty():
		_failures.append("server_started が飛んでこない")
		controller.queue_free()
		return

	# サーバーが起動して書き出すまで待つ。
	for _attempt in range(40):
		await create_timer(0.1).timeout
		if FileAccess.file_exists(log_path) and FileAccess.get_file_as_string(log_path).length() > 0:
			break

	var log_text := FileAccess.get_file_as_string(log_path)
	if log_text.is_empty():
		_failures.append("ログファイルに何も書かれていない: %s" % log_path)
	elif not log_text.contains(str(GAME_PORT)):
		_failures.append("ログに待受ポートが出てこない:\n%s" % log_text)

	controller.stop_server()
	controller.queue_free()
	await process_frame


## 起動後に落ちたことに気付くこと。
##
## 気付けないと「SERVER OFFLINE — RETRYING」としか出ず、通信の問題なのか
## サーバーが死んだのか区別できない。
func _check_death_is_noticed() -> void:
	var controller = HostServerController.new()
	root.add_child(controller)
	controller.server_failed.connect(
		func(text: String) -> void: _failures.append("起動できなかった: %s" % text)
	)
	var exit_codes: Array[int] = []
	controller.server_exited.connect(func(code: int) -> void: exit_codes.append(code))

	controller.start_server(GAME_PORT)
	await process_frame
	if not controller.owns_server():
		_failures.append("生存監視の検査用にサーバーを起動できなかった")
		controller.queue_free()
		return

	# 外から落とす。CREATE ROOM 側は stop_server を通らない。
	OS.kill(controller.server_pid)
	for _attempt in range(40):
		await create_timer(0.1).timeout
		if not exit_codes.is_empty():
			break

	if exit_codes.is_empty():
		_failures.append("サーバーが落ちても server_exited が飛んでこない")
	if controller.owns_server():
		_failures.append("落ちたあともプロセスを掴んだままになっている")

	controller.stop_server()
	controller.queue_free()
	await process_frame


## バイナリが見つからないとき、どこを探したかを伝えること。
func _check_missing_binary_message_lists_paths() -> void:
	var controller = HostServerController.new()
	var candidates: Array[String] = controller._server_executable_candidates()
	if candidates.is_empty():
		_failures.append("探索候補が空。どこを探したか伝えられない")
	for candidate in candidates:
		if not candidate.is_absolute_path():
			_failures.append("候補が絶対パスでない: %s" % candidate)
	controller.free()
