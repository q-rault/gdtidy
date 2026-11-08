extends Node  # expect: file-name
#comment without a space  # expect: comment-space
#print("commented-out code")
#region Section
#endregion
#---------
# fine


func _helper() -> void:
	pass


func calls(other: Node) -> void:
	_helper()
	self._helper()
	other._helper()  # expect: private-call
	Inner._make()
	super._ready()


class Inner:
	static func _make() -> void:
		pass
