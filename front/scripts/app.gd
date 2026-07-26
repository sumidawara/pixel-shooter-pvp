extends Node

@onready var menu_screen := %MenuScreen
@onready var game_screen := %GameScreen
@onready var host_server := %HostServerController

var hosting_room := false
var joined_room := false
var local_player_id := 0
var pending_host_settings: Dictionary = {}


func _ready() -> void:
	get_window().title = "Pixel Shooter PvP"
	menu_screen.join_requested.connect(_on_join_requested)
	menu_screen.cancel_connection_requested.connect(_cancel_join_attempt)
	menu_screen.create_requested.connect(_on_create_requested)
	menu_screen.add_cpu_requested.connect(NetworkClient.add_cpu)
	menu_screen.remove_cpu_requested.connect(NetworkClient.remove_cpu)
	menu_screen.start_match_requested.connect(NetworkClient.start_match)
	menu_screen.room_settings_changed.connect(NetworkClient.update_room_settings)
	menu_screen.leave_room_requested.connect(_leave_room)
	menu_screen.quit_requested.connect(_quit_game)
	game_screen.exit_requested.connect(_leave_room)
	host_server.server_started.connect(_on_local_server_started)
	host_server.server_failed.connect(_on_local_server_failed)
	NetworkClient.status_changed.connect(_on_status_changed)
	NetworkClient.welcome_received.connect(_on_welcome_received)
	NetworkClient.rejected.connect(_on_rejected)
	NetworkClient.snapshot_received.connect(_on_snapshot_received)
	_show_menu()
	if OS.get_environment("PIXEL_SHOOTER_AUTOCONNECT") == "1":
		menu_screen.request_connection()


func _on_join_requested(server_url: String, player_name: String) -> void:
	hosting_room = false
	joined_room = false
	menu_screen.set_connecting(true)
	var normalized_url := server_url.strip_edges()
	if _is_matchmaker_url(normalized_url):
		NetworkClient.connect_via_matchmaker(_as_http_url(normalized_url), player_name)
	else:
		NetworkClient.connect_to_server(normalized_url, player_name)


func _cancel_join_attempt() -> void:
	NetworkClient.disconnect_from_server()
	joined_room = false
	local_player_id = 0


# Docker Composeの標準Matchmakerポートへ誤ってws://を付けても、
# HTTPのマッチングAPIとして扱い、WebSocketの無限再試行を避ける。
func _is_matchmaker_url(url: String) -> bool:
	var lower := url.to_lower()
	return (
		lower.begins_with("http://")
		or lower.begins_with("https://")
		or _url_uses_port(lower, NetworkConfig.MATCHMAKER_PORT)
	)


func _as_http_url(url: String) -> String:
	var lower := url.to_lower()
	if lower.begins_with("ws://"):
		return "http://" + url.substr(5)
	if lower.begins_with("wss://"):
		return "https://" + url.substr(6)
	if not lower.begins_with("http://") and not lower.begins_with("https://"):
		return "http://" + url
	return url


func _url_uses_port(url: String, port: int) -> bool:
	var authority_start := url.find("://")
	authority_start = authority_start + 3 if authority_start >= 0 else 0
	var path_start := url.find("/", authority_start)
	var authority := (
		url.substr(authority_start)
		if path_start < 0
		else url.substr(authority_start, path_start - authority_start)
	)
	return authority.ends_with(":%d" % port)


func _on_create_requested(player_name: String, port: int, settings: Dictionary) -> void:
	hosting_room = true
	joined_room = false
	pending_host_settings = settings
	host_server.start_server(port)
	NetworkClient.player_name = player_name.strip_edges()


func _on_local_server_started(url: String) -> void:
	await get_tree().create_timer(0.35).timeout
	NetworkClient.connect_to_server(url, NetworkClient.player_name)


func _on_local_server_failed(reason: String) -> void:
	hosting_room = false
	menu_screen.set_connecting(false)
	menu_screen.set_status(reason)


func _on_welcome_received(player_id: int, reconnected: bool) -> void:
	local_player_id = player_id
	joined_room = true
	menu_screen.set_connecting(false)
	menu_screen.show_room(hosting_room, NetworkClient.server_url)
	if hosting_room and not pending_host_settings.is_empty():
		NetworkClient.update_room_settings(pending_host_settings)
	if game_screen.visible and reconnected:
		game_screen.resume_session(player_id)


func _on_snapshot_received(snapshot: Dictionary) -> void:
	if not joined_room:
		return
	var next_phase := str(snapshot.get("phase", "waiting"))
	if next_phase == "waiting":
		if game_screen.visible:
			game_screen.end_session()
		menu_screen.visible = true
		game_screen.visible = false
		menu_screen.show_room(hosting_room, NetworkClient.server_url)
		menu_screen.apply_room_snapshot(
			snapshot.get("players", []),
			snapshot.get("room", {}),
			local_player_id
		)
	elif not game_screen.visible:
		game_screen.start_session(local_player_id)
		menu_screen.visible = false
		game_screen.visible = true


func _on_rejected(reason: String) -> void:
	if hosting_room:
		host_server.stop_server()
	hosting_room = false
	joined_room = false
	local_player_id = 0
	game_screen.end_session()
	game_screen.visible = false
	menu_screen.visible = true
	menu_screen.set_connecting(false)
	# URLを直してすぐ再試行できるよう、タイトルへ戻さずJoin画面を維持する。
	menu_screen.show_join()
	menu_screen.set_status(reason)


func _on_status_changed(text: String) -> void:
	menu_screen.set_status(text)
	game_screen.set_connection_status(text)


func _leave_room() -> void:
	NetworkClient.disconnect_from_server()
	game_screen.end_session()
	if hosting_room:
		host_server.stop_server()
	hosting_room = false
	joined_room = false
	local_player_id = 0
	pending_host_settings.clear()
	_show_menu()


func _quit_game() -> void:
	_leave_room()
	get_tree().quit()


func _show_menu() -> void:
	menu_screen.visible = true
	menu_screen.set_connecting(false)
	menu_screen.show_title()
	game_screen.visible = false
