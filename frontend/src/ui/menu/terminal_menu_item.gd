extends HBoxContainer

## 端末風メニューの1行。枠を常時出さず、いま選んでいる行だけに印を出す。
##
## 選択肢を全部シアンの矩形で囲うと、文字より枠の面積のほうが大きくなり、
## 視線が「何を選ぶか」ではなく箱そのものに引かれる。どれも同じ強さで主張するので、
## 結果としてどれも強調されていない状態になる。
##
## 端末では普通、選択肢を矩形で囲わず、いま居る行を印で示す。この行はその形に合わせ、
## 強いシアンをフォーカスとホバーのときだけ出す。
##
## 印を独立したLabelにしているのは、ボタンの文字へ ">" を足すと足したぶん行が
## 横へずれて、選ぶたびに文字が動いて見えるため。印の欄は常に確保しておく。

## 選択中に出す印。BACKのような戻る操作では "<" を使う。
@export var marker := ">"

@onready var marker_label: Label = $Marker

var button: Button


func _ready() -> void:
	button = _find_button()
	if button == null:
		push_error("terminal menu item %s has no button" % name)
		return
	for signal_name in ["focus_entered", "focus_exited", "mouse_entered", "mouse_exited"]:
		button.connect(signal_name, _refresh)
	_refresh()


## いまこの行を選んでいるか。
##
## 押せない行では出さない。押せないのに選べるように見えるのは嘘になる。
func is_active() -> bool:
	return button != null and not button.disabled and (button.has_focus() or button.is_hovered())


func _refresh() -> void:
	marker_label.text = marker if is_active() else ""


## 行の中のボタン。名前で探さないのは、ボタン側の名前が画面ごとに違い、
## その名前で `%` 参照されているため。
func _find_button() -> Button:
	for child in get_children():
		if child is Button:
			return child
	return null
