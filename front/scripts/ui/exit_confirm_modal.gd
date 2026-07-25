extends CanvasLayer

signal exit_confirmed

@onready var cancel_button: Button = %CancelButton
@onready var exit_button: Button = %ExitButton


func _ready() -> void:
	cancel_button.pressed.connect(close_modal)
	exit_button.pressed.connect(_confirm_exit)


func open_modal() -> void:
	visible = true
	cancel_button.grab_focus()


func close_modal() -> void:
	visible = false


func is_open() -> bool:
	return visible


func _confirm_exit() -> void:
	close_modal()
	exit_confirmed.emit()
