# gdlint skips `class_name` followed by `extends` on the same line; gdtidy
# checks it like any other `class_name`.
class_name Bad_Name extends Node  # expect: class-name
