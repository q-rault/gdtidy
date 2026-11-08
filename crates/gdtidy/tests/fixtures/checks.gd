extends Node
## A fixture for the rules on by default that are not about names or limits.
## @tutorial: https://docs.godotengine.org
## @tutorial(Docs): https://docs.godotengine.org
## @tutorial https://docs.godotengine.org  # expect: doc-tag
## @param x unknown tags are shown as text  # expect: doc-tag
## [codeblock]
## @anything inside a code block is code
## [/codeblock]

## @deprecated: use B.
## @experimental
## @deprecated use B  # expect: doc-tag
const A = preload("res://a.gd")
const B = preload("res://a.gd")  # expect: duplicated-load
var c = load("res://c.gd")
var d = load('res://c.gd')  # expect: duplicated-load


func compare(x: int, y: int) -> bool:
	if x == x:  # expect: comparison-with-itself
		print(x)
	if (x + 1) < (x+1):  # expect: comparison-with-itself
		print(x)
	return x < y


func assign(x: int) -> void:
	x = x  # expect: self-assignment
	self.position = self.position  # expect: self-assignment
	x = -x
	x += x
	print(x)


func loops(items: Array) -> void:
	for item in items:
		item = 1  # expect: loop-variable-assignment
		item += 2  # expect: loop-variable-assignment
		var f := func(item): item = 3
		for other in items:
			item = other  # expect: loop-variable-assignment
		for item in items:
			item = 4  # expect: loop-variable-assignment
		print(f)


func passes(x: int) -> int:
	if x:
		pass  # expect: unnecessary-pass
		print(x)
	else:
		pass
	pass  # expect: unnecessary-pass
	return x


func elif_after_return(x: int) -> int:
	if x > 1:
		return 1
	elif x > 0:  # expect: no-elif-return
		return 2
	elif x < -5:  # expect: no-elif-return
		print(x)
	elif x < -10:
		return 3
	return 0


func else_after_return(x: int) -> int:
	if x > 1:
		return 1
	else:  # expect: no-else-return
		x += 1
	return x


func else_keeps_its_variable(x: int) -> int:
	var y = 1
	if x > 1:
		return y
	else:
		var y = 2
		return y


func nested_if_returns(x: int) -> int:
	if x > 1:
		if x > 2:
			return 2
		else:  # expect: no-else-return
			return 1
	else:  # expect: no-else-return
		return 0


func match_returns(x: int) -> int:
	if x > 1:
		match x:
			2:
				return 2
			_:
				return 3
	else:  # expect: no-else-return
		x = 0
	return x


class Holder:
	pass  # expect: unnecessary-pass
	var value = 1
