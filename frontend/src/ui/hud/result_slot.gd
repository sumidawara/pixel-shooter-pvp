extends Control

## 表彰台1人分の表示。台の上に立つ順位と、台に乗らない順位の両方で使う。
@export var show_box := true

@onready var box: Panel = %Box
@onready var figure: TextureRect = %Figure
@onready var rank_label: Label = %RankLabel
@onready var name_label: Label = %NameLabel
@onready var score_label: Label = %ScoreLabel


func _ready() -> void:
	box.visible = show_box


func apply_entry(entry: Dictionary, color: Color) -> void:
	visible = true
	var display_name := str(entry.get("name", "PLAYER"))
	if bool(entry.get("is_cpu", false)):
		display_name += "*"
	name_label.text = display_name
	name_label.modulate = color
	score_label.text = "%d PTS" % int(entry.get("score", 0))
	rank_label.text = str(int(entry.get("rank", 0)))
	figure.modulate = color


func clear() -> void:
	visible = false
