extends Control

signal connect_requested(server_url: String, player_name: String)

const DEFAULT_SERVER_URL := "ws://127.0.0.1:9001"
const CURSOR_TEXTURE: Texture2D = preload("res://assets/art/cursor.png")

@onready var name_input: LineEdit = %PlayerNameInput
@onready var server_input: LineEdit = %ServerUrlInput
@onready var connect_button: Button = %ConnectButton
@onready var status_label: Label = %StatusLabel


func _ready() -> void:
	Input.set_custom_mouse_cursor(CURSOR_TEXTURE, Input.CURSOR_ARROW, Vector2(12, 12))
	name_input.text = "Player-%03d" % (OS.get_process_id() % 1000)
	var environment_url := OS.get_environment("PIXEL_SHOOTER_SERVER_URL")
	server_input.text = environment_url if not environment_url.is_empty() else DEFAULT_SERVER_URL
	connect_button.pressed.connect(request_connection)
	server_input.text_submitted.connect(func(_value: String): request_connection())


func request_connection() -> void:
	connect_requested.emit(server_input.text, name_input.text)


func set_connecting(connecting: bool) -> void:
	connect_button.disabled = connecting
	connect_button.text = "CONNECTING..." if connecting else "CONNECT"


func set_status(text: String) -> void:
	status_label.text = text
