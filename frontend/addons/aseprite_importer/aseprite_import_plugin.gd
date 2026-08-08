@tool
extends EditorImportPlugin

## .aseprite をそのままテクスチャとして読み込むインポータ。
##
## 書き出し済みPNGを持たずに済ませるのが目的。生成物は .godot/imported/ へ入り、
## Git管理外なので、新規チェックアウトでは godot --import で作り直される。

const AsepriteDocument := preload("res://addons/aseprite_importer/aseprite_document.gd")


func _get_importer_name() -> String:
	return "pixel_shooter.aseprite"


func _get_visible_name() -> String:
	return "Aseprite Texture"


func _get_recognized_extensions() -> PackedStringArray:
	return PackedStringArray(["aseprite", "ase"])


func _get_save_extension() -> String:
	return "res"


func _get_resource_type() -> String:
	return "Texture2D"


func _get_priority() -> float:
	return 1.0


func _get_import_order() -> int:
	return 0


func _get_preset_count() -> int:
	return 1


func _get_preset_name(_preset_index: int) -> String:
	return "Default"


func _get_import_options(_path: String, _preset_index: int) -> Array[Dictionary]:
	return []


func _get_option_visibility(_path: String, _option: StringName, _options: Dictionary) -> bool:
	return true


func _import(
	source_file: String,
	save_path: String,
	_options: Dictionary,
	_platform_variants: Array[String],
	_gen_files: Array[String]
) -> Error:
	var result := AsepriteDocument.load_from_file(source_file)
	if not result.error.is_empty():
		push_error("aseprite import: %s" % result.error)
		return ERR_FILE_CORRUPT
	if result.image == null:
		push_error("aseprite import: 画像を生成できなかった: %s" % source_file)
		return ERR_FILE_CORRUPT

	# ドット絵なので可逆で保存する。拡大時の補間はプロジェクト既定の
	# テクスチャフィルタ（Nearest）に従う。
	var texture := PortableCompressedTexture2D.new()
	texture.create_from_image(
		result.image, PortableCompressedTexture2D.COMPRESSION_MODE_LOSSLESS)
	return ResourceSaver.save(texture, "%s.%s" % [save_path, _get_save_extension()])
