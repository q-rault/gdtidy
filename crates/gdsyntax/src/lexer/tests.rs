use super::*;

/// The significant tokens (what Godot's parser sees), as `KIND` or
/// `KIND:text`, with trivia left out.
fn significant(src: &str) -> Vec<String> {
    let lexed = tokenize(src);
    let mut offset = 0;
    let mut out = Vec::new();
    for token in &lexed.tokens {
        let text = &src[offset..offset + token.len as usize];
        offset += token.len as usize;
        if token.kind.is_trivia() {
            continue;
        }
        let name = format!("{:?}", token.kind);
        out.push(if text.is_empty() || text == "\n" || text == "\r\n" {
            name
        } else {
            format!("{name}:{text}")
        });
    }
    assert_eq!(offset, src.len(), "tokens must cover the whole source");
    out
}

fn errors(src: &str) -> Vec<String> {
    tokenize(src)
        .errors
        .into_iter()
        .map(|e| e.message)
        .collect()
}

#[test]
fn statements_end_with_newline_tokens() {
    assert_eq!(
        significant("var a = 1\n"),
        ["VAR_KW:var", "IDENTIFIER:a", "EQUAL:=", "INT:1", "NEWLINE"]
    );
}

#[test]
fn a_file_without_final_line_break_still_ends_its_statement() {
    assert_eq!(significant("pass"), ["PASS_KW:pass", "NEWLINE"]);
}

#[test]
fn blocks_get_indent_and_dedent() {
    assert_eq!(
        significant("func f():\n\tpass\nvar x\n"),
        [
            "FUNC_KW:func",
            "IDENTIFIER:f",
            "PARENTHESIS_OPEN:(",
            "PARENTHESIS_CLOSE:)",
            "COLON::",
            "NEWLINE",
            "INDENT",
            "PASS_KW:pass",
            "NEWLINE",
            "DEDENT",
            "VAR_KW:var",
            "IDENTIFIER:x",
            "NEWLINE",
        ]
    );
}

#[test]
fn the_end_of_the_file_closes_open_blocks() {
    assert_eq!(
        significant("if a:\n\tif b:\n\t\tpass"),
        [
            "IF_KW:if",
            "IDENTIFIER:a",
            "COLON::",
            "NEWLINE",
            "INDENT",
            "IF_KW:if",
            "IDENTIFIER:b",
            "COLON::",
            "NEWLINE",
            "INDENT",
            "PASS_KW:pass",
            "NEWLINE",
            "DEDENT",
            "DEDENT",
        ]
    );
}

#[test]
fn blank_and_comment_lines_do_not_change_indentation() {
    assert_eq!(
        significant("func f():\n\n# note\n\tpass\n"),
        [
            "FUNC_KW:func",
            "IDENTIFIER:f",
            "PARENTHESIS_OPEN:(",
            "PARENTHESIS_CLOSE:)",
            "COLON::",
            "NEWLINE",
            "INDENT",
            "PASS_KW:pass",
            "NEWLINE",
            "DEDENT",
        ]
    );
}

#[test]
fn line_breaks_inside_brackets_are_trivia() {
    assert_eq!(
        significant("var a = [\n\t1,\n\t2,\n]\n"),
        [
            "VAR_KW:var",
            "IDENTIFIER:a",
            "EQUAL:=",
            "BRACKET_OPEN:[",
            "INT:1",
            "COMMA:,",
            "INT:2",
            "COMMA:,",
            "BRACKET_CLOSE:]",
            "NEWLINE",
        ]
    );
}

#[test]
fn backslash_joins_lines() {
    assert_eq!(
        significant("var a = 1 + \\\n\t\t2\n"),
        [
            "VAR_KW:var",
            "IDENTIFIER:a",
            "EQUAL:=",
            "INT:1",
            "PLUS:+",
            "INT:2",
            "NEWLINE"
        ]
    );
    assert_eq!(
        errors("var a = 1 \\ 2\n"),
        ["Expected new line after \"\\\"."]
    );
}

#[test]
fn a_sign_joins_the_number_only_where_no_operand_comes_before() {
    assert_eq!(
        significant("x = -1"),
        ["IDENTIFIER:x", "EQUAL:=", "INT:-1", "NEWLINE"]
    );
    assert_eq!(
        significant("x - 1"),
        ["IDENTIFIER:x", "MINUS:-", "INT:1", "NEWLINE"]
    );
}

#[test]
fn numbers() {
    for (src, kind) in [
        ("0x1F", "INT"),
        ("0b101", "INT"),
        ("1_000_000", "INT"),
        ("1.5", "FLOAT"),
        (".5", "FLOAT"),
        ("1.", "FLOAT"),
        ("1e10", "FLOAT"),
        ("2.5e-3", "FLOAT"),
    ] {
        assert_eq!(
            significant(src),
            [format!("{kind}:{src}"), "NEWLINE".into()],
            "{src}"
        );
    }
    assert_eq!(
        significant("1..2")[..3],
        ["INT:1", "PERIOD_PERIOD:..", "INT:2"]
    );
    assert_eq!(
        errors("1__0"),
        ["Multiple underscores cannot be adjacent in a numeric literal."]
    );
    assert_eq!(errors("0x"), ["Expected hexadecimal digit after \"0x\"."]);
    assert_eq!(errors("12abc"), ["Invalid numeric notation."]);
}

#[test]
fn strings() {
    for (src, kind) in [
        ("\"a\"", "STRING"),
        ("'a'", "STRING"),
        ("\"\"\"a\n\"b\"\n\"\"\"", "STRING"),
        ("r\"C:\\path\\file\"", "STRING"),
        // In a raw string, `\"` keeps the quote in the string.
        ("r\"say \\\"hi\\\"\"", "STRING"),
        ("&\"name\"", "STRING_NAME"),
        ("^\"a/b\"", "NODE_PATH"),
        ("\"spans\nlines\"", "STRING"),
    ] {
        assert_eq!(
            significant(src),
            [format!("{kind}:{src}"), "NEWLINE".into()],
            "{src}"
        );
    }
    assert_eq!(errors("var s = \"never closed\n"), ["Unterminated string."]);
    // As in Godot, a raw string cannot end with a backslash.
    assert_eq!(errors("r\"C:\\path\\\""), ["Unterminated string."]);
    assert_eq!(errors("\"bad \\q\""), ["Invalid escape in string."]);
}

#[test]
fn keywords_names_and_annotations() {
    assert_eq!(
        significant("@export var match: int = PI"),
        [
            "ANNOTATION:@export",
            "VAR_KW:var",
            "MATCH_KW:match",
            "COLON::",
            "IDENTIFIER:int",
            "EQUAL:=",
            "CONST_PI:PI",
            "NEWLINE",
        ]
    );
    assert_eq!(significant("_")[0], "UNDERSCORE:_");
    assert_eq!(significant("tést_日本語")[0], "IDENTIFIER:tést_日本語");
    assert_eq!(significant("true")[0], "TRUE_KW:true");
}

#[test]
fn operators_and_node_paths() {
    assert_eq!(
        significant("$%A/%B ** 2 != 1 -> x"),
        [
            "DOLLAR:$",
            "PERCENT:%",
            "IDENTIFIER:A",
            "SLASH:/",
            "PERCENT:%",
            "IDENTIFIER:B",
            "STAR_STAR:**",
            "INT:2",
            "BANG_EQUAL:!=",
            "INT:1",
            "FORWARD_ARROW:->",
            "IDENTIFIER:x",
            "NEWLINE",
        ]
    );
}

#[test]
fn conflict_markers() {
    assert_eq!(
        significant("<<<<<<< HEAD")[0],
        "VCS_CONFLICT_MARKER:<<<<<<<"
    );
    assert_eq!(significant("a << b")[1], "LESS_LESS:<<");
}

#[test]
fn indentation_errors_match_godot() {
    assert_eq!(
        errors("func f():\n\tif x:\n\t\tpass\n\t    pass\n"),
        ["Mixed use of tabs and spaces for indentation."]
    );
    // A mixed line that also lands between levels gets both errors, as in Godot.
    assert_eq!(
        errors("func f():\n\tif x:\n\t\tpass\n\t  pass\n"),
        [
            "Mixed use of tabs and spaces for indentation.",
            "Unindent doesn't match the previous indentation level."
        ]
    );
    assert_eq!(
        errors("func f():\n\tpass\nfunc g():\n    pass\n"),
        ["Used space character for indentation instead of tab as used before in the file."]
    );
    assert_eq!(
        errors("func f():\n\t\tpass\n\tpass\n"),
        ["Unindent doesn't match the previous indentation level."]
    );
}

#[test]
fn bracket_errors() {
    assert_eq!(
        errors("a)"),
        ["Closing \")\" doesn't have an opening counterpart."]
    );
    // Godot drops the unmatched `(` before looking for an opening bracket.
    assert_eq!(
        errors("(a]"),
        ["Closing \"]\" doesn't have an opening counterpart."]
    );
    assert_eq!(
        errors("[(a]"),
        ["Closing \"]\" doesn't match the opening \"[\"."]
    );
}

#[test]
fn byte_order_mark_and_crlf() {
    let src = "\u{feff}func f():\r\n\tpass\r\n";
    assert_eq!(
        significant(src),
        [
            "FUNC_KW:func",
            "IDENTIFIER:f",
            "PARENTHESIS_OPEN:(",
            "PARENTHESIS_CLOSE:)",
            "COLON::",
            "NEWLINE",
            "INDENT",
            "PASS_KW:pass",
            "NEWLINE",
            "DEDENT",
        ]
    );
    assert!(errors(src).is_empty());
}

#[test]
fn invalid_characters() {
    assert_eq!(errors("a ¤ b"), ["Invalid character \"¤\" (U+00A4)."]);
    assert_eq!(
        errors("a\u{a0}b"),
        ["Invalid white space character U+00A0."]
    );
}

#[test]
fn empty_and_comment_only_files() {
    assert!(significant("").is_empty());
    assert!(errors("").is_empty());
    assert_eq!(significant("# only a comment"), ["NEWLINE"]);
}
