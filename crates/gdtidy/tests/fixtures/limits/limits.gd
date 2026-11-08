extends Node  # expect: max-public-methods


func public_one() -> void:
	pass


# gdlint leaves static functions out of its function limits; gdtidy does not.
static func public_static(a, b, c) -> int:  # expect: function-arguments-number
	return a + b + c


func _private(a, b) -> void:
	print(a, b)


func lambda_returns_are_not_counted(x: int) -> int:
	if x > 1:
		return 2
	var f := func(): return 5
	print(f)
	return 0


func three_returns(x: int) -> int:
	if x > 1:
		return 2
	if x > 0:
		return 1
	return 0  # expect: max-returns


func long_function(x: int) -> void:  # expect: max-function-lines
	# Comments and blank lines are not counted.

	print(x)
	print(x)
	print(x)
	print(x)
	print(x)
	print(x)
	print(x)


func many_branches(x: int) -> void:  # expect: max-branches
	if x > 1:
		pass
	elif x > 0:
		pass
	else:
		pass


func deep(x: int) -> void:
	for i in x:
		while i > 0:
			if i == 1:  # expect: max-nesting-depth
				if i:
					pass


func match_and_branch_are_one_level(x: int) -> void:
	match x:
		1:
			if x:
				pass


class Inner:  # expect: max-public-methods
	func a() -> void:
		pass

	func b() -> void:
		pass

	func c() -> void:
		pass
