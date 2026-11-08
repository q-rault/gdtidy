# gdtidy lint rules

Every rule gdtidy can report. `gdtidy rules` lists them with whether they are on by default, and `gdtidy rules --format json` adds each rule's concern and options. [RFC 0002](../rfcs/0002-lint-rules.md) lists the rules still to come, and where gdlint's other rules went.

Options are set in `gdtidy.toml`:

```toml
[lint.rules.max-line-length]
max = 130
```

## max-line-length

On by default. A line is longer than `max` columns.

| Option | Default | Meaning |
|---|---|---|
| `max` | 100 | The longest line allowed, in columns |
| `tab-width` | 4 | How many columns a tab counts for, as in Godot's editor. gdlint counts 1, and `gdtidy migrate` keeps that for converted projects |

## unknown-rule-in-comment

On by default. A silencing comment names a rule gdtidy does not know, so it silences nothing. Names of gdlint rules that moved to another tool, and GDQuest's names for gdtidy rules (`private-access`, `enum-member-name`, `variable-name`), are accepted.
