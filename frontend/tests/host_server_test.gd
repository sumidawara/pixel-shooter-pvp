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
	await _check_control_port_collision_is_reported()
	await _check_server_starts_and_writes_a_log()
	await _check_death_is_noticed()
	_check_missing_binary_message_lists_paths()

	if not _failures.is_empty():
		push_error("host server:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("host server: 失敗経路と生存監視が期待どおりだった")
	quit(0)


## 制御APIのポートだけが埋まっている場合も、起動前に気付くこと。
##
## 以前は --debug-bind を渡しておらず制御APIは常に9101だった。衝突しても
## サーバーは起動を続けるため、管理機能だけが黙って死んでいた。
func _check_control_port_collision_is_reported() -> void:
	var control_port := GAME_PORT + HostServerController.CONTROL_PORT_OFFSET
	var blocker := TCPServer.new()
	if blocker.listen(control_port, NetworkConfig.LOCAL_SERVER_HOST) != OK:
		_failures.append("検査用にポート%dを確保できなかった" % control_port)
		return

	var controller = HostServerController.new()
	root.add_child(controller)
	# GDScriptのラムダはローカル変数を値でキャプチャするため、
	# 受け取った値は配列などの参照型へ入れないと外から読めない。
	var reasons: Array[String] = []
	controller.server_failed.connect(func(text: String) -> void: reasons.append(text))
	controller.start_server(GAME_PORT)
	await process_frame

	if reasons.is_empty():
		_failures.append("制御APIのポートが埋まっているのに起動しようとした")
	elif not reasons[0].contains(str(control_port)):
		_failures.append("どのポートが埋まっているか示していない: %s" % reasons[0])
	if controller.owns_server():
		_failures.append("起動に失敗したのにプロセスを掴んでいる")

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
