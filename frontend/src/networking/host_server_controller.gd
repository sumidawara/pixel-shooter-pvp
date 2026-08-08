extends Node

## CREATE ROOM で同梱のRustサーバーを子プロセスとして起動する。
##
## 子プロセスは画面もログも持たないため、失敗したときに何も分からなくなりやすい。
## 起動できない理由、落ちたこと、ログの場所は、いずれもここから伝える。

signal server_started(url: String)
signal server_failed(reason: String)
## 起動には成功したが、その後に落ちた。
signal server_exited(exit_code: int)
## 希望のポートが埋まっていたので、別の番号で開いた。
signal port_changed(requested: int, used: int)

## 制御APIの待受ポートは、ゲーム用ポートからこの値だけずらして決める。
## 既定の 9001 / 9101 と同じ関係にしてある。
const CONTROL_PORT_OFFSET := 100
## 生存確認の間隔。落ちたことに気付くのが目的なので、細かく見る必要はない。
const LIVENESS_CHECK_SECONDS := 0.5
## サーバーが接続先を書き出すのを待つ上限。
##
## 書き出しは待受の直後なので普通は一瞬で終わる。ここまで待って現れないなら
## 起動に失敗している。待ち続けるとルーム画面で無言のまま固まる。
const ADDRESS_WAIT_SECONDS := 5.0

var server_pid := -1
var log_path := ""

var _liveness_left := 0.0
var _requested_port := 0
var _address_path := ""
var _address_wait_left := 0.0


func start_server(port: int) -> void:
	stop_server()
	if OS.has_feature("web"):
		server_failed.emit("CREATE ROOM IS NOT AVAILABLE IN WEB BUILDS")
		return

	var server_path := _find_server_executable()
	if server_path.is_empty():
		server_failed.emit(
			"RUST SERVER BINARY WAS NOT FOUND\nLOOKED IN:\n  %s"
			% "\n  ".join(_server_executable_candidates())
		)
		return

	# ポートが埋まっていたときに空きを探すかどうかは、サーバー側の
	# server.json（network.port_search_range）が決める。ここでは探さない。
	# 両方が探すと、server.json を書き換えても効かない側が勝ってしまう。
	# 実際に開けた番号は、サーバーが --address-file へ書いたものを読む。
	_requested_port = port
	_address_path = _address_file_path()
	# 前回の結果が残っていると、それを今回の接続先と読み違える。
	DirAccess.remove_absolute(_address_path)

	log_path = _log_path()
	server_pid = OS.create_process(
		server_path,
		[
			"--bind", NetworkConfig.local_server_bind_address(port),
			"--debug-bind", NetworkConfig.local_server_bind_address(port + CONTROL_PORT_OFFSET),
			"--log-file", log_path,
			"--address-file", _address_path,
		],
		false
	)
	if server_pid <= 0:
		server_pid = -1
		server_failed.emit("COULD NOT START THE RUST SERVER\n%s" % server_path)
		return
	_liveness_left = LIVENESS_CHECK_SECONDS
	_address_wait_left = ADDRESS_WAIT_SECONDS


func stop_server() -> void:
	_address_wait_left = 0.0
	if server_pid > 0:
		OS.kill(server_pid)
		server_pid = -1


func owns_server() -> bool:
	return server_pid > 0


func _process(delta: float) -> void:
	if server_pid <= 0:
		return
	if _address_wait_left > 0.0:
		_wait_for_address(delta)
		return
	_liveness_left -= delta
	if _liveness_left > 0.0:
		return
	_liveness_left = LIVENESS_CHECK_SECONDS
	if OS.is_process_running(server_pid):
		return
	# 落ちた。放っておくと「SERVER OFFLINE — RETRYING」としか出ず、
	# 通信の問題なのかサーバーが死んだのか区別できない。
	var exit_code := OS.get_process_exit_code(server_pid)
	server_pid = -1
	server_exited.emit(exit_code)


func _exit_tree() -> void:
	stop_server()


## サーバーが接続先を書き出すのを待ち、書かれたら起動完了として扱う。
##
## 待受に成功して初めて書かれるため、この文字列が出てきたことが
## 「本当に繋がる状態になった」ことの証拠になる。
func _wait_for_address(delta: float) -> void:
	var address := _read_address()
	if not address.is_empty():
		_address_wait_left = 0.0
		var used_port := address.get_slice(":", 1).to_int()
		if used_port != _requested_port:
			# 番号が変わったことは伝える。他の人が入るときに必要になる。
			port_changed.emit(_requested_port, used_port)
		server_started.emit("ws://%s" % address)
		return

	# 書き出す前に死んだ＝待受できなかった。理由はログにある。
	if not OS.is_process_running(server_pid):
		var exit_code := OS.get_process_exit_code(server_pid)
		server_pid = -1
		_address_wait_left = 0.0
		server_failed.emit(
			"THE SERVER COULD NOT OPEN PORT %d (EXIT %d)\nSEE %s"
			% [_requested_port, exit_code, log_path]
		)
		return

	_address_wait_left -= delta
	if _address_wait_left <= 0.0:
		stop_server()
		server_failed.emit(
			"THE SERVER DID NOT REPORT AN ADDRESS WITHIN %d SECONDS\nSEE %s"
			% [int(ADDRESS_WAIT_SECONDS), log_path]
		)


## サーバーが書いた接続先。まだ無ければ、または書き込み途中なら空文字。
##
## ファイルは中身より先に作られるため、そのまま読むと空や欠けた文字列を
## 接続先として掴んでしまう。形が揃うまでは「まだ無い」として扱う。
func _read_address() -> String:
	if not FileAccess.file_exists(_address_path):
		return ""
	var text := FileAccess.get_file_as_string(_address_path).strip_edges()
	if text.get_slice_count(":") != 2:
		return ""
	if text.get_slice(":", 1).to_int() <= 0:
		return ""
	return text


## サーバーが実際の接続先を書き出す先。
func _address_file_path() -> String:
	return ProjectSettings.globalize_path("user://server_address.txt")


## サーバーのログの置き場所。
##
## 配布版では子プロセスの出力がどこにも出ないため、ファイルへ残す。
## user:// はプラットフォームごとの書き込み可能な場所を指す。
func _log_path() -> String:
	return ProjectSettings.globalize_path("user://server.log")


func _server_executable_candidates() -> Array[String]:
	var executable_name := (
		"pixel-shooter-server.exe" if OS.has_feature("windows") else "pixel-shooter-server"
	)
	var executable_dir := OS.get_executable_path().get_base_dir()
	var candidates: Array[String] = []
	if OS.has_feature("macos") and not OS.has_feature("editor"):
		candidates.append(executable_dir.path_join(executable_name))
	else:
		candidates.append(executable_dir.path_join("server").path_join(executable_name))
		candidates.append(executable_dir.path_join(executable_name))
	if OS.has_feature("editor"):
		candidates.append(ProjectSettings.globalize_path("res://../target/debug/" + executable_name))
		candidates.append(ProjectSettings.globalize_path("res://../target/release/" + executable_name))
	return candidates


func _find_server_executable() -> String:
	for candidate in _server_executable_candidates():
		if FileAccess.file_exists(candidate):
			return candidate
	return ""
