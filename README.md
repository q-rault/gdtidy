# gdtidy

Formats and lints GDScript, following the [official Godot style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html). It is one fast binary with no Python, and is meant to replace `gdformat` and `gdlint` in [gdsuite](https://github.com/q-rault/gdsuite).

> **Status:** `gdtidy lint` has gdlint's rules, with the same findings on 14 public projects, plus 9 more ([RFC 0002](rfcs/0002-lint-rules.md)); [docs/rules.md](docs/rules.md) lists them all. `gdtidy fmt` is not written yet.

## Install

```sh
cargo install --path crates/gdtidy
```

## Usage

```sh
gdtidy lint                  # the whole project
gdtidy lint player/ hud.gd   # only these files and folders
gdtidy lint --format json    # for tools and agents
gdtidy lint --format sarif   # for GitHub code scanning
gdtidy lint --fix            # repair what is safe to repair, report the rest
gdtidy migrate               # convert ./gdlintrc into gdtidy.toml
gdtidy rules                 # every rule gdtidy can report (see docs/rules.md)
gdtidy fmt                   # not implemented yet
```

When walking a folder, gdtidy skips hidden folders, `addons`, folders with a `.gdignore`, and nested Godot projects, as the editor does. A file or folder you name directly is always checked.

### Configuration

`gdtidy.toml` sits next to `project.godot` (or in any folder under it). For each script, gdtidy uses the nearest one above it, never one outside the project:

```toml
[lint]
disable = ["max-line-length"]   # rules that are on by default
enable = []                     # rules that are off by default
exclude = ["generated"]         # folders to skip, as in .gitignore

[lint.rules.max-line-length]
max = 130
```

An unknown rule, option or key is an error, so a typo never turns a rule off. In `exclude`, a name alone (`generated`) is a folder at any depth, and a path (`tools/generated`, `/generated`) is relative to the `gdtidy.toml`.

### Moving from gdlint

`gdtidy migrate` reads `gdlintrc` (or `.gdlintrc`) and writes a `gdtidy.toml` next to it that reports the same problems. It keeps the values the project changed, keeps gdlint's tab width for `max-line-length`, and says where each dropped rule went, such as `unused-argument`, which Godot reports. The gdlintrc is left as it is. Rule names and silencing comments are the same, so nothing else needs to change.

Differences with gdlint, all deliberate, are listed in [docs/gdlint-differences.md](docs/gdlint-differences.md).

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
| `0` | No problems |
| `1` | Problems found |
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

The defaults follow the style guide: tabs, two blank lines around functions, enums with one member per line, trailing commas in multi-line collections, and double quotes. Two choices where the guide and common practice disagree will be settings:

- **Continuation indent** for a wrapped call: the guide uses two tabs, but most projects use one.
- **Spaces inside a one-line dictionary**: the guide writes `{ "a": 1 }`, but most projects write `{"a": 1}`.

Line breaks you write inside brackets are kept, so a list you split stays split.

## Crates

| Crate | What it is |
| --- | --- |
| [`gdtidy`](crates/gdtidy) | The command-line tool |
| [`gdsyntax`](https://github.com/q-rault/gdsyntax) | A GDScript parser translated from Godot's own, with a lossless tree, shared with the other gdsuite tools. It has its own repository; clone it next to this one to build gdtidy. |

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
