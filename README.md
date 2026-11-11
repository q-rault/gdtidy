# gdtidy

Formats and lints GDScript, following the [official Godot style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html). It is one fast binary with no Python, and is meant to replace `gdformat` and `gdlint` in [gdsuite](https://github.com/q-rault/gdsuite).

> **Status:** `gdtidy lint` has gdlint's rules, with the same findings on 14 public projects, plus 9 more ([RFC 0002](rfcs/0002-lint-rules.md)); [the rules page](https://q-rault.github.io/gdtidy/docs/rules) lists them all. `gdtidy fmt` formats to the style guide without ever changing what the code does ([RFC 0003](rfcs/0003-formatter.md)); [the formatting page](https://q-rault.github.io/gdtidy/docs/formatting) lists every change it makes.

Documentation: **[q-rault.github.io/gdtidy](https://q-rault.github.io/gdtidy/)**

## Install

Download a binary from the [releases page](https://github.com/q-rault/gdtidy/releases), or:

```sh
cargo install gdtidy
```

## Usage

```sh
gdtidy fmt                   # format the whole project in place
gdtidy fmt --check           # change nothing; list the files that would change
gdtidy fmt --diff            # change nothing; show the changes
gdtidy fmt -                 # format standard input to standard output
gdtidy lint                  # the whole project
gdtidy lint player/ hud.gd   # only these files and folders
gdtidy lint --format json    # for tools and agents
gdtidy lint --format sarif   # for GitHub code scanning
gdtidy lint --fix            # repair what is safe to repair, report the rest
gdtidy migrate               # convert ./gdlintrc and ./gdformatrc into gdtidy.toml (or gdsuite.toml)
gdtidy rules                 # every rule gdtidy can report
```

When walking a folder, gdtidy skips hidden folders, `addons`, folders with a `.gdignore`, and nested Godot projects, as the editor does. A file or folder you name directly is always checked.

### Configuration

`gdtidy.toml` sits next to `project.godot` (or in any folder under it). For each script, gdtidy uses the nearest one above it, never one outside the project.

A project that uses [gdsuite](https://github.com/q-rault/gdsuite) can instead keep the same `[lint]` and `[fmt]` sections in `gdsuite.toml`, next to the other tools' settings. In each folder, a `gdsuite.toml` with a `[lint]` or `[fmt]` section comes first. Keep gdtidy's settings in one file: a `gdtidy.toml` beside such a `gdsuite.toml` is ignored, with a warning, and the two are not merged.

```toml
[lint]
disable = ["max-line-length"]   # rules that are on by default
enable = []                     # rules that are off by default
exclude = ["generated"]         # folders to skip, as in .gitignore

[lint.rules.max-line-length]
max = 130

[fmt]
exclude = ["generated"]         # folders not to format
continuation-indent = 2         # levels for a continued line (1 is common too)
dict-brace-spaces = true        # { "a": 1 }; false gives {"a": 1}
trailing-commas = true          # add them to multi-line arrays, dictionaries and enums
```

An unknown rule, option or key is an error, so a typo never turns a rule off. In `exclude`, a name alone (`generated`) is a folder at any depth, and a path (`tools/generated`, `/generated`) is relative to the `gdtidy.toml`.

### Moving from gdtoolkit

`gdtidy migrate` reads `gdlintrc` and `gdformatrc` (with or without a leading dot) and writes a `gdtidy.toml` next to them, or adds the sections to the `gdsuite.toml` there when the project has one and no `gdtidy.toml`:
- its `[lint]` section reports the same problems as gdlint. It keeps the values the project changed, keeps gdlint's tab width for `max-line-length`, and says where each dropped rule went, such as `unused-argument`, which Godot reports. Rule names and silencing comments are the same, so nothing else needs to change.
- its `[fmt]` section keeps gdformat's excluded folders and its `{"a": 1}` dictionaries. gdformat's `line_length` has no equivalent, because gdtidy does not wrap lines.

The original files are left as they are. If `gdtidy.toml` (or `gdsuite.toml`) exists, only the sections it does not have yet are added, so a project converted from gdlint can convert its gdformatrc later. `--force` replaces a whole `gdtidy.toml`, but never touches a section already in `gdsuite.toml`.

Differences with gdlint, all deliberate, are listed in [Moving from gdtoolkit](https://q-rault.github.io/gdtidy/docs/migrate-from-gdtoolkit#differences-with-gdlint).

### Formatting

`gdtidy fmt` follows the style guide: tabs, two blank lines around functions and classes, one space around operators, `and`/`or`/`not`, double quotes, enums one member per line, trailing commas in multi-line collections, and more ([formatting](https://q-rault.github.io/gdtidy/docs/formatting)). It keeps the line breaks you write and never wraps lines. Every result is parsed again and must have the same code as before, or the file is left alone and reported, so formatting can never change what a script does.

To keep a hand-aligned part of a file as it is:

```gdscript
# gdtidy: fmt-off
const TABLE = [
	[1,   0],
	[0,  10],
]
# gdtidy: fmt-on
```

### Fixing

`gdtidy lint --fix` repairs `unnecessary-pass` and `comment-space`, then reports what is left. A fix is applied only if the fixed file still parses and has the same code apart from what the fix removes; otherwise the file is left as it is.

### Silencing a rule

```gdscript
var x = 1  # gdtidy: ignore=rule-name     (this line and the next)
# gdtidy: disable=rule-name               (until enable, or the end of the file)
# gdtidy: enable=rule-name
```

gdlint's comments (`# gdlint: ignore=...`) and GDQuest's (`# gdlint-ignore-next-line ...`) work too, so a project moving from either needs no edits.

A file with a syntax error is skipped, not reported as a problem: syntax errors are Godot's to report, so in gdsuite they come from gdtypecheck. gdtidy still lists the file and Godot's message on stderr (and under `skipped` in JSON), and exits with `2`, so a run that left files out never looks clean.

| Exit code | Meaning |
| --- | --- |
| `0` | No problems (`fmt`: formatted, or nothing to change) |
| `1` | Problems found (`fmt --check` or `--diff`: some files would change) |
| `2` | The check could not run, or some files could not be checked |

## Scope

Each gdsuite tool answers one question, and a problem belongs to the first tool in this list that can see it:

1. **`gdtidy fmt`**: layout that can be fixed without changing what the code does, and without a judgement call.
2. **gdtypecheck**: anything Godot's compiler reports, or that needs types, the engine or other files' resources.
3. **gdtest**: anything that needs the code to run.
4. **gdhealth**: anything that needs several files, git history or project-wide figures. gdhealth measures; it does not fail a build on a single function's size.
5. **`gdtidy lint`**: everything else that one file's syntax can answer, such as naming, member order and per-function limits.

So `gdtidy lint` never reports what `gdtidy fmt` would fix, nor what Godot already warns about. Every rule declares the concern it covers (`gdtidy rules --format json`), so gdsuite can check that no two tools claim the same one.

## Style

The defaults follow the style guide. Two choices where the guide and common practice disagree are settings:

- **Continuation indent** for a line continued after `foo(a,` or a `\`: the guide uses two levels (`continuation-indent = 2`), but many projects use one. A bracket that ends its line, as in `foo(` followed by the arguments on their own lines, always indents its content by one level, as the guide does for arrays.
- **Spaces inside a one-line dictionary**: the guide writes `{ "a": 1 }`, but most projects write `{"a": 1}` (`dict-brace-spaces = false`).

Line breaks you write inside brackets are kept, so a list you split stays split.

## Crates

| Crate | What it is |
| --- | --- |
| [`gdtidy`](crates/gdtidy) | The command-line tool |
| [`gdsyntax`](https://github.com/q-rault/gdsyntax) | A GDScript parser translated from Godot's own, with a lossless tree, shared with the other gdsuite tools. It has its own repository and is on crates.io. |

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
