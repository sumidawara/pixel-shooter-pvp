extends Node

signal server_started(url: String)
signal server_failed(reason: String)

var server_pid := -1


func start_server(port: int) -> void:
	stop_server()
	if OS.has_feature("web"):
		server_failed.emit("CREATE ROOM IS NOT AVAILABLE IN WEB BUILDS")
		return
	var port_probe := TCPServer.new()
	if port_probe.listen(port, "127.0.0.1") != OK:
		server_failed.emit(
			"PORT %d IS ALREADY IN USE — STOP THE OTHER SERVER OR JOIN IT" % port
		)
		return
	port_probe.stop()
	var server_path := _find_server_executable()
	if server_path.is_empty():
		server_failed.emit("RUST SERVER BINARY WAS NOT FOUND")
		return
	server_pid = OS.create_process(
		server_path,
		["--bind", "127.0.0.1:%d" % port],
		false
	)
	if server_pid <= 0:
		server_pid = -1
		server_failed.emit("COULD NOT START THE RUST SERVER")
		return
	server_started.emit("ws://127.0.0.1:%d" % port)


func stop_server() -> void:
	if server_pid > 0:
		OS.kill(server_pid)
		server_pid = -1


func owns_server() -> bool:
	return server_pid > 0


func _exit_tree() -> void:
	stop_server()


func _find_server_executable() -> String:
	var executable_name := "pixel-shooter-server.exe" if OS.has_feature("windows") else "pixel-shooter-server"
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
	for candidate in candidates:
		if FileAccess.file_exists(candidate):
			return candidate
	return ""
