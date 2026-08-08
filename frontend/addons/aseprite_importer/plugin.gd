@tool
extends EditorPlugin

const AsepriteImportPlugin := preload("res://addons/aseprite_importer/aseprite_import_plugin.gd")

var _import_plugin: EditorImportPlugin


func _enter_tree() -> void:
	_import_plugin = AsepriteImportPlugin.new()
	add_import_plugin(_import_plugin)


func _exit_tree() -> void:
	if _import_plugin != null:
		remove_import_plugin(_import_plugin)
		_import_plugin = null
