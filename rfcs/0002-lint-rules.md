# RFC 0002: Lint rules for gdtidy

Status: proposed, 2026-11-08. Target: replace `gdlint` (gdtoolkit 4.x) in gdsuite.

## Summary

Give `gdtidy lint` the rules that replace gdlint, minus what other tools already own: a configuration file, comments that silence a rule, automatic fixes for the safe cases, and text, JSON and SARIF output. The rules run on the typed tree from gdsyntax 0.1.

Once this RFC is done, gdsuite can run `gdtidy lint` instead of `gdlint`, and gdlint's Python dependency is down to `gdformat` alone. RFC 0003 (the formatter) removes that one.

## Goals

1. **Same findings as gdlint, for every rule kept.** On the comparison corpus (see Testing), every difference with gdlint is either a gdlint bug or a documented choice.
2. **No overlap.** No rule reports what Godot already warns about (gdtypecheck's job) or what `gdtidy fmt` would fix. Each rule declares its concern, and `gdtidy rules --format json` publishes them.
3. **Switching costs nothing.** Kept rules keep gdlint's names. Existing `# gdlint: ignore=...` comments keep working. `gdtidy migrate` converts a `gdlintrc` so the same project reports the same problems.
4. **Fast.** Files are checked in parallel. The whole comparison corpus (533,000 lines) is linted in under 1 second on 8 threads.

## Non-goals

- **Rules that need types, the engine, other files or running the code.** By the scope order in the README, those belong to gdtypecheck, gdtest or gdhealth.
- **New rules beyond gdlint's.** Candidates are listed at the end; each gets its own short RFC once the set below ships.
- **Formatting.** That is RFC 0003.
- **Godot syncing.** The `godot-sync` script belongs to gdsyntax, which now has its own repository.

## Where each gdlint rule goes

gdlint 4.x has 30 rules. 25 stay in gdtidy lint, 3 move to Godot (and so to gdtypecheck), and 2 move to the formatter.

### Kept in gdtidy lint (25)

| gdlint rule | Default | Concern | Notes |
|---|---|---|---|
| `function-name` | `(_on_PascalCase(_snake)*\|_?snake_case)` | `naming:function` | |
| `class-name` | `PascalCase` | `naming:class` | |
| `sub-class-name` | `_?PascalCase` | `naming:inner-class` | |
| `signal-name` | `snake_case` | `naming:signal` | |
| `class-variable-name` | `_?snake_case` | `naming:member-variable` | Static and `@export` variables included, as in gdlint |
| `class-load-variable-name` | `PascalCase\|_?snake_case` | `naming:member-load-variable` | A variable set to `load()`/`preload()` |
| `function-variable-name` | `snake_case` | `naming:local-variable` | |
| `function-preload-variable-name` | `PascalCase` | `naming:local-preload-variable` | |
| `function-argument-name` | `_?snake_case` | `naming:parameter` | Lambda parameters included |
| `loop-variable-name` | `_?snake_case` | `naming:loop-variable` | |
| `enum-name` | `PascalCase` | `naming:enum` | |
| `enum-element-name` | `UPPER_SNAKE_CASE` | `naming:enum-value` | |
| `constant-name` | `_?UPPER_SNAKE_CASE` | `naming:constant` | |
| `load-constant-name` | `PascalCase\|_?UPPER_SNAKE_CASE` | `naming:load-constant` | |
| `class-definitions-order` | gdlint's 14 sections | `member-order` | See below |
| `max-returns` | 6 | `limit:function-returns` | |
| `function-arguments-number` | 10 | `limit:function-parameters` | |
| `max-public-methods` | 20 | `limit:class-public-methods` | Per class, inner classes separately |
| `max-file-lines` | 1000 | `limit:file-lines` | gdhealth measures file size but never fails on it |
| `max-line-length` | 100 | `limit:line-length` | See below |
| `duplicated-load` | on | `duplicated-load` | Same path loaded twice in one file |
| `comparison-with-itself` | on | `comparison-with-itself` | `x == x` and the like; Godot does not warn |
| `no-elif-return` | on | `control-flow:elif-after-return` | |
| `no-else-return` | on | `control-flow:else-after-return` | |
| `unnecessary-pass` | on | `unnecessary-pass` | `pass` in a block that has other statements; fixable |

Every kept rule keeps its gdlint ID.

### Moved to Godot, reported by gdtypecheck (3)

| gdlint rule | Godot 4.7 equivalent |
|---|---|
| `unused-argument` | Warning `UNUSED_PARAMETER` |
| `expression-not-assigned` | Warning `STANDALONE_EXPRESSION` (plus `STANDALONE_TERNARY`; from 4.8, strings used as comments too) |
| `mixed-tabs-and-spaces` | Parse error "Mixed use of tabs and spaces for indentation." |

### Moved to `gdtidy fmt` (2)

| gdlint rule | Why it is the formatter's |
|---|---|
| `trailing-whitespace` | Removing it never changes what the code does |
| `tab-characters` | In gdlint this is the tab width for `max-line-length`, not a rule. It becomes that rule's `tab-width` option, below |

Until RFC 0003 ships, gdsuite keeps running `gdformat --check`, which already covers these.

### Two decisions inside kept rules

**`max-line-length` counts a tab as 4 columns by default.** gdlint counts a tab as one character (`tab-characters: 1`), but Godot's editor shows it as 4 columns. The option is `tab-width`. `gdtidy migrate` writes gdlintrc's value, or 1 if there is none, so a converted project gets the same results.

**`class-definitions-order` keeps gdlint's sections by default.** Those are: tools, class_name, extends, doc comment, signals, enums, constants, static variables, exports, public variables, private variables, `@onready` public, `@onready` private, then everything else. They match the official style guide's items 1 to 11. The guide goes on to order methods: `_static_init`, static methods, built-in virtual methods in engine order, other virtual methods, overrides, other methods, inner classes. That part becomes a separate rule, `method-order`, off by default, because turning it on would flag nearly every existing project.

## Configuration

`gdtidy.toml`, found by walking up from the file being linted. The walk stops at the first folder that has a `gdtidy.toml` or a `project.godot`. Without one, every rule above runs with its default.

```toml
[lint]
# Rules not to run.
disable = ["max-public-methods"]
# Extra folders to skip. Hidden folders, addons, .gdignore folders and nested
# projects are always skipped, as now.
exclude = ["reports"]

[lint.rules.max-line-length]
max = 130
tab-width = 1

[lint.rules.function-name]
pattern = '(_on_[A-Z][A-Za-z0-9]*(_[a-z0-9]+)*|_?[a-z][a-z0-9]*(_[a-z0-9]+)*)'

[lint.rules.class-definitions-order]
sections = ["tools", "classnames", "extends", "docstrings", "signals", "enums",
            "consts", "staticvars", "exports", "pubvars", "prvvars",
            "onreadypubvars", "onreadyprvvars", "others"]
```

- An unknown rule name or option is an error (exit 2), so a typo never silently turns a rule off.
- Patterns are full-match regular expressions, as in gdlint, compiled once per run with the `regex` crate.
- `gdtidy rules --format json` lists each rule's options with their defaults and types, so gdsuite can check the file too.

### Migrating from gdlint

`gdtidy migrate [gdlintrc]` reads a `gdlintrc` (YAML) and writes `gdtidy.toml`:
- Kept rules and their values carry over.
- `excluded_directories` becomes `exclude`.
- Each dropped rule is listed with where it went, for example "unused-argument: reported by Godot as UNUSED_PARAMETER (gdtypecheck)".
- The original file is left untouched.

gdsuite's own `migrate` command calls it.

## Silencing a rule

The same forms as gdlint, under either name:

```gdscript
var x = 1  # gdtidy: ignore=function-variable-name
# gdtidy: ignore=max-line-length          (silences it on the next line)
# gdtidy: disable=max-returns             (until the matching enable, or the end of file)
# gdtidy: enable=max-returns
```

- **gdlint's spellings:** `# gdlint: ignore=...` and its `disable`/`enable` forms are read too, so existing projects need no edits.
- **Comma-separated lists** are accepted, as in gdlint.
- **Unknown names:** a comment naming a rule gdtidy does not have is reported once, as `unknown-rule-in-comment` (concern `suppression-comments`). The exception is a gdlint rule that moved to another tool; that is silently accepted, so old comments don't become noise.
- **Comments are found as tokens,** not with a regex over the text, so `# gdtidy:` inside a string never counts.

## Fixes

`gdtidy lint --fix` applies fixes that cannot change what the code does, then reports what is left.

- **In this RFC:** `unnecessary-pass` (delete the statement and its line). Later candidates: `no-else-return` and `no-elif-return`, which dedent the else branch.
- **Safety:** a fix applies only to a file that parses without errors. The fixed text must parse without errors too. Apart from the removed nodes, it must give the same tree, ignoring trivia. Otherwise the file is left alone and the problem is reported as not fixed.
- **Idempotence:** running `--fix` twice changes nothing the second time. A test checks this on the whole corpus.

## Output

- **Text** (default): `path:line:col: warning[rule]: message`, with skipped files on stderr as now.
- **JSON:** the current version-1 report with `diagnostics` and `skipped`. Each diagnostic gains `concern` and `fixable`.
- **SARIF 2.1.0**, with `--format sarif`: one `run`, one `reportingDescriptor` per rule (id, concern, short description, help URL) and one `result` per diagnostic. Skipped files become `toolExecutionNotifications`. GitHub code scanning accepts it as is.

All rules report `warning`. As now, the exit code is 1 when there are problems and 2 when files were skipped or the configuration is invalid.

## Design

- **The rule interface.** Each rule is a function that takes the `SourceFile`, the source text and its own options, and returns diagnostics. Most rules walk the typed tree. Line rules (`max-line-length`, `max-file-lines`) read the text. Rules don't share state.
- **The registry.** One table holds every rule's ID, concern, summary, default options, fixability and gdlint aliases. `gdtidy rules` and the SARIF descriptors come from it.
- **Parallelism.** Files are read and linted on a `rayon` thread pool, and results are sorted by path so the output order never depends on timing.
- **Skipped files.** A file with a syntax error is still skipped without running rules (decided in the previous change), so rules only ever see trees without `Error` nodes.

## Testing

1. **Per-rule fixtures:** for each rule, a `.gd` file of cases that must be reported and cases that must not. Expected results are written inline, as `# expect: rule-name` comments.
2. **Comparison with gdlint:** a test (`GDTIDY_PARITY=1`, off by default) runs both tools with the same configuration on:
   - gdtoolkit's own valid scripts;
   - the 14 public projects used for gdsyntax;
   - any private projects named in `GDSYNTAX_CORPUS`.

   Every difference has to be listed in `tests/parity-differences.md` with its reason, such as a gdlint bug, Godot 4 syntax gdlint mis-parses, or the `tab-width` default.
3. **Migration:** gdlintrc files with every option convert to configurations that give the same results.
4. **Fixes:** applying `--fix` to the corpus never adds an error, never changes the tree beyond the fix, and is idempotent.
5. **Speed:** the corpus is linted in under 1 second on 8 threads, in the benchmark example.

## Milestones

Each milestone ends with a commit.

| | Content | Done when |
|---|---|---|
| M1 | Rule interface and registry, `gdtidy.toml` loading, silencing comments (both spellings), parallel run, SARIF output | Framework tests pass with one sample rule; `gdtidy rules --format json` lists options |
| M2 | The 14 naming rules | Fixtures pass; parity on names |
| M3 | `class-definitions-order`, the limits (`max-returns`, `function-arguments-number`, `max-public-methods`, `max-file-lines`, `max-line-length`) | Fixtures pass; parity |
| M4 | `duplicated-load`, `comparison-with-itself`, `no-elif-return`, `no-else-return`, `unnecessary-pass`, and `--fix` | Fixtures and fix-safety tests pass |
| M5 | `gdtidy migrate`, the full parity run, README and docs | Every parity difference documented; corpus under 1 s |

**After M5:**
- publish gdtidy 0.1.0 on crates.io (needs approval);
- in gdsuite's repository, run `gdtidy lint` instead of `gdlint`, read gdtidy's `skipped` list so syntax errors appear once, and convert `gdlintrc` during `gdsuite migrate`.

## Later rule candidates

Not part of this RFC. Each one needs its own scope check first:
- `method-order` (the style guide's method order, described above);
- `consider-using-in` (`x == a or x == b`);
- simplifiable boolean expressions;
- `@onready` variables that are never assigned a node path;
- magic numbers.

Godot's own warnings (`UNUSED_VARIABLE`, `UNTYPED_DECLARATION` and the other 48) stay gdtypecheck's, whatever their name suggests.
