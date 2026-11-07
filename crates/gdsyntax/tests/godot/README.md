# Godot's GDScript test scripts

These are the scripts in `modules/gdscript/tests/scripts` of the Godot Engine, copied unchanged from the tag named by each folder (`4.7` is `4.7-stable`). Each `.gd` file has an `.out` file whose first line says what Godot does with it: `GDTEST_OK`, `GDTEST_PARSER_ERROR`, `GDTEST_ANALYZER_ERROR` or `GDTEST_RUNTIME_ERROR`. For a parser error, the second line is Godot's message.

gdsyntax's `tests/godot_suite.rs` checks its tokenizer and parser against them.

They are Godot's work, under the MIT license in [LICENSE.txt](LICENSE.txt).

To add a Godot version, copy its scripts into a new folder:

```sh
git -C <godot> archive <tag> modules/gdscript/tests/scripts | tar -x -C crates/gdsyntax/tests/godot/<version> --strip-components=4
```
