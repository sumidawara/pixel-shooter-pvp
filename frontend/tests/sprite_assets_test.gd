extends SceneTree

## Aseprite原本がテクスチャとして読み込め、実際に見える状態かを検証する。
##
## 以前は書き出し済みPNGをコミットしており、書き出しに失敗しても
## PNG自体は生成されるため透明なまま気付かず入りうる状態だった
## （実際に lalokinpoppos.png が alpha=7/255 で入っていた）。
## 現在は原本だけを管理し、addons/aseprite_importer が読み込む。
## 書き出し工程が無くなったぶん事故は減るが、インポータの不具合や
## 未対応機能の混入は起こりうるので、ここで見張る。
##
##     godot --headless --path frontend --script res://tests/sprite_assets_test.gd

const ASEPRITE_ROOT := "res://assets/aseprite"
## アプリアイコンの元にしている原本。
const ICON_SOURCE := "res://assets/aseprite/actors/player/player_stand.aseprite"

## 「見えている」とみなす最低のalpha。
## 半分でも不透明な画素が1つも無いスプライトは、読み込みが壊れていると判断する。
const MINIMUM_VISIBLE_ALPHA := 128

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var sources := _find_aseprite_files(ASEPRITE_ROOT)
	if sources.is_empty():
		push_error("sprite assets: %s に .aseprite が1つも無い" % ASEPRITE_ROOT)
		quit(1)
		return

	for path in sources:
		_check(path)
	_check_application_icon()

	if not _failures.is_empty():
		push_error(
			"sprite assets: 読み込めない、または画面に出ないアセットがある:\n  "
			+ "\n  ".join(_failures)
			+ "\naddons/aseprite_importer が対応していない機能を使っていないか確認すること"
		)
		quit(1)
		return

	print("sprite assets: %d 件すべてがテクスチャとして読め、不透明な画素があった" % sources.size())
	quit(0)


## アプリアイコンだけはAseprite原本を直接使えないので、別に見張る。
##
## `config/icon` と各エクスポートプリセットの `application/icon` は、エンジンが
## 生の画像ファイルとして読む。カスタムインポータを通したリソースは指定できないため、
## ここだけPNGを持つ。原本と二重管理になるので、内容が一致することを検査する。
func _check_application_icon() -> void:
	var icon_path: String = ProjectSettings.get_setting("application/config/icon", "")
	if icon_path.is_empty():
		_failures.append("application/config/icon が設定されていない")
		return

	# エンジンと同じ経路で読めること。.aseprite を指すと読めずに黙って落ちる。
	var icon := Image.new()
	if icon.load(icon_path) != OK:
		_failures.append(
			"%s: エンジンが画像として読めない。アイコンはPNG等の生の画像である必要がある"
			% icon_path)
		return

	var source: Texture2D = load(ICON_SOURCE)
	if source == null:
		_failures.append("%s: アイコンの元にする原本を読めない" % ICON_SOURCE)
		return
	var expected := source.get_image()
	if expected.is_compressed():
		expected.decompress()
	expected.convert(Image.FORMAT_RGBA8)
	icon.convert(Image.FORMAT_RGBA8)

	if icon.get_size() != expected.get_size():
		_failures.append("%s: 寸法が原本と違う（%s と %s）"
			% [icon_path, icon.get_size(), expected.get_size()])
		return
	for y in range(icon.get_height()):
		for x in range(icon.get_width()):
			var a := icon.get_pixel(x, y)
			var b := expected.get_pixel(x, y)
			# 完全に透明な画素のRGBは見えないので比較しない。
			if absf(a.a - b.a) > 0.004 or (a.a > 0.0 and (
				absf(a.r - b.r) > 0.004 or absf(a.g - b.g) > 0.004 or absf(a.b - b.b) > 0.004)):
				_failures.append(
					"%s: 原本 %s と内容が違う（(%d,%d) で相違）。原本を変えたらアイコンも書き出し直すこと"
					% [icon_path, ICON_SOURCE, x, y])
				return


func _find_aseprite_files(directory: String) -> PackedStringArray:
	var found := PackedStringArray()
	for name in DirAccess.get_directories_at(directory):
		found.append_array(_find_aseprite_files(directory.path_join(name)))
	for name in DirAccess.get_files_at(directory):
		# インポート後は .import が並ぶので、原本だけを拾う。
		if name.get_extension() in ["aseprite", "ase"]:
			found.append(directory.path_join(name))
	return found


func _check(path: String) -> void:
	if not ResourceLoader.exists(path):
		# 原本を足したのにインポートされていない状態。
		# 以前 ghost.aseprite がマニフェスト未登録で宙に浮いていた事例がある。
		_failures.append("%s: リソースとして認識されていない" % path)
		return
	var texture: Texture2D = load(path)
	if texture == null:
		_failures.append("%s: テクスチャとして読み込めない" % path)
		return
	var image := texture.get_image()
	if image == null:
		_failures.append("%s: 画像を取り出せない" % path)
		return
	if image.is_compressed():
		image.decompress()
	image.convert(Image.FORMAT_RGBA8)

	# get_pixelを1画素ずつ呼ぶと大きな絵で遅いため、生バイトを直接見る。
	var data := image.get_data()
	var largest_alpha := 0
	var index := 3
	while index < data.size():
		largest_alpha = maxi(largest_alpha, data[index])
		if largest_alpha >= 255:
			break
		index += 4

	if largest_alpha < MINIMUM_VISIBLE_ALPHA:
		_failures.append(
			"%s: 最大alphaが %d しかない（%dx%d）。ほぼ透明で画面に出ない"
			% [path, largest_alpha, image.get_width(), image.get_height()]
		)
