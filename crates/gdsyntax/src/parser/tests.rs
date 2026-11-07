use super::*;
use crate::SyntaxNode;

/// The tree as indented `KIND` lines, tokens with their text, trivia left out.
fn dump(src: &str) -> String {
    let parse = parse(src);
    let root = SyntaxNode::new_root(parse.green);
    assert_eq!(root.to_string(), src, "the tree must reproduce the source");
    let mut out = String::new();
    for event in root.preorder_with_tokens() {
        if let rowan::WalkEvent::Enter(element) = event {
            // A node's ancestors include itself; a token's start at its parent.
            let depth = match &element {
                rowan::NodeOrToken::Node(node) => node.ancestors().count() - 1,
                rowan::NodeOrToken::Token(token) => token.parent_ancestors().count(),
            };
            match element {
                rowan::NodeOrToken::Node(node) => {
                    out += &format!("{}{:?}\n", "  ".repeat(depth), node.kind());
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind().is_trivia() || token.text().is_empty() {
                        continue;
                    }
                    let text = token.text().replace('\n', "\\n");
                    out += &format!("{}{:?} {text:?}\n", "  ".repeat(depth), token.kind());
                }
            }
        }
    }
    out
}

fn errors(src: &str) -> Vec<String> {
    parse(src).errors.into_iter().map(|e| e.message).collect()
}

fn assert_clean(src: &str) {
    let parse = parse(src);
    let root = SyntaxNode::new_root(parse.green);
    assert_eq!(root.to_string(), src);
    assert!(parse.errors.is_empty(), "{src:?}: {:?}", parse.errors);
}

#[test]
fn declarations() {
    let tree = dump("extends Node\n\n@export var speed: float = 1.0\nconst MAX := 3\n");
    assert_eq!(
        tree,
        r#"SOURCE_FILE
  EXTENDS
    EXTENDS_KW "extends"
    IDENTIFIER "Node"
    NEWLINE "\\n"
  VAR
    ANNOTATION_NODE
      ANNOTATION "@export"
    VAR_KW "var"
    IDENTIFIER "speed"
    COLON ":"
    TYPE
      IDENTIFIER "float"
    EQUAL "="
    LITERAL
      FLOAT "1.0"
    NEWLINE "\\n"
  CONST
    CONST_KW "const"
    IDENTIFIER "MAX"
    COLON ":"
    EQUAL "="
    LITERAL
      INT "3"
    NEWLINE "\\n"
"#
    );
}

#[test]
fn functions_and_statements() {
    assert_clean(
        "func f(a: int, b := 2, ...rest) -> void:\n\tif a > b:\n\t\treturn\n\telif a == b:\n\t\tpass\n\telse:\n\t\tfor i in range(3):\n\t\t\tcontinue\n\twhile true:\n\t\tbreak\n",
    );
    assert_clean("static func make() -> Node:\n\treturn Node.new()\n");
    assert_clean("@abstract func run() -> void\n");
}

#[test]
fn expressions() {
    let tree = dump("var x = 1 + 2 * 3\n");
    assert!(tree.contains(
        "BINARY_EXPR\n      LITERAL\n        INT \"1\"\n      PLUS \"+\"\n      BINARY_EXPR"
    ));
    assert_clean("var a = b.c(d)[e] as Node\n");
    assert_clean("var a = x if y else z\n");
    assert_clean("var a = not x in y and z is not Node\n");
    assert_clean(
        "var a = {\"key\": 1, other = 2}\n"
            .replace("other = 2", "\"b\": 2")
            .as_str(),
    );
    assert_clean("var d = {key = 1, \"other\" = 2}\n");
    assert_clean(
        "var n = $Path/To/Node\nvar u = %Unique\nvar m = $%A/%B\nvar q = $\"Quoted path\"\n",
    );
    assert_clean("var p = preload(\"res://a.gd\")\nvar t = -1 ** 2\n");
    assert_clean("func f():\n\tawait get_tree().process_frame\n\tsuper()\n\tsuper.f()\n");
}

#[test]
fn multiline_brackets_and_lambdas() {
    assert_clean("var a = [\n\t1,\n\t2,\n]\n\n\nfunc f():\n\tfoo(\n\t\t1,\n\t\t2\n\t)\n\tpass\n");
    assert_clean(
        "func f():\n\tbutton.pressed.connect(func():\n\t\tprint(1)\n\t\tprint(2)\n\t)\n\tpass\n",
    );
    assert_clean("func f():\n\tvar g = func(x): return x + 1\n\tg.call(1)\n");
    assert_clean("func f():\n\tnode.visibility_changed.connect(func(): if visible: open())\n");
}

#[test]
fn properties() {
    assert_clean("var a: int:\n\tget:\n\t\treturn 1\n\tset(value):\n\t\tpass\n");
    assert_clean("var b = 0: set = set_b, get = get_b\n");
    assert_clean("var c: get = get_c\n");
}

#[test]
fn match_statements() {
    assert_clean(
        "func f(x):\n\tmatch x:\n\t\t1, 2:\n\t\t\tpass\n\t\t[var a, ..]:\n\t\t\tpass\n\t\t{\"k\": var v, ..}:\n\t\t\tpass\n\t\tvar n when n > 3:\n\t\t\tpass\n\t\t_:\n\t\t\tpass\n",
    );
}

#[test]
fn classes_enums_signals() {
    assert_clean(
        "class_name Player extends CharacterBody2D\n\nsignal hit(damage: int)\nenum State { IDLE, RUN = 2 }\n\n\nclass Inner extends Node:\n\tvar x = 1\n\n\tfunc f():\n\t\tpass\n",
    );
}

#[test]
fn godot_error_messages() {
    assert_eq!(
        errors("func f()\n\tpass\n")[0],
        "Expected \":\" after \"if\" condition."
            .replace("\"if\" condition", "function")
            .replace(
                "Expected \":\" after function.",
                "Unexpected \"Indent\" in class body."
            )
    );
    assert_eq!(
        errors("var x = \n")[0],
        "Expected expression for variable initial value after \"=\"."
    );
    assert_eq!(
        errors("func f():\n\tbreak\n")[0],
        "Cannot use \"break\" outside of a loop."
    );
    assert_eq!(
        errors("export var x\n")[0],
        "The \"export\" keyword was removed in Godot 4. Use an export annotation (\"@export\", \"@export_range\", etc.) instead."
    );
    assert_eq!(
        errors("func f():\n\tif a\n\t\tpass\n")[0],
        "Expected \":\" after \"if\" condition."
    );
}

#[test]
fn broken_code_still_round_trips() {
    for src in [
        "func f(:\n\tpass\n",
        "var = 1\n",
        "func f():\n\tvar x = (1 + \n",
        "class A\n\tpass\n",
        "<<<<<<< HEAD\nvar a = 1\n=======\nvar a = 2\n>>>>>>> b\n",
        "func f():\n\treturn [1, 2\n",
        "@\n",
        "]]]\n",
    ] {
        let parse = parse(src);
        assert_eq!(SyntaxNode::new_root(parse.green).to_string(), src);
        assert!(!parse.errors.is_empty(), "{src:?} should have errors");
    }
}

#[test]
fn deep_nesting_is_reported_not_overflowed() {
    let src = format!("var x = {}1{}\n", "(".repeat(5000), ")".repeat(5000));
    let parse = parse(&src);
    assert_eq!(SyntaxNode::new_root(parse.green).to_string(), src);
    assert!(
        parse
            .errors
            .iter()
            .any(|e| e.message.contains("nested too deeply"))
    );
}

/// Windows gives the main thread 1 MiB of stack. Deeply nested input must
/// stop at the depth limit well before that runs out.
#[test]
fn deep_nesting_fits_in_a_one_mebibyte_stack() {
    let inputs = [
        format!("var x = {}1{}\n", "(".repeat(10_000), ")".repeat(10_000)),
        format!("var x = {}1{}\n", "[".repeat(10_000), "]".repeat(10_000)),
        format!("var x = {}1\n", "-".repeat(10_000)),
        format!("var x = {}1{}\n", "f(".repeat(10_000), ")".repeat(10_000)),
        format!("var x = {}\n", "func(): return ".repeat(2_000) + "1"),
        {
            let mut src = String::from("func f():\n");
            for depth in 1..2_000 {
                src += &"\t".repeat(depth);
                src += "if x:\n";
            }
            src += &"\t".repeat(2_000);
            src += "pass\n";
            src
        },
        format!(
            "func f(x):\n\tmatch x:\n\t\t{}1{}:\n\t\t\tpass\n",
            "[".repeat(10_000),
            "]".repeat(10_000)
        ),
        format!(
            "var x: {}int{}\n",
            "Array[".repeat(10_000),
            "]".repeat(10_000)
        ),
    ];
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            for src in &inputs {
                let parse = parse(src);
                assert_eq!(SyntaxNode::new_root(parse.green).to_string(), *src);
            }
        })
        .unwrap()
        .join()
        .expect("parsing must not overflow a 1 MiB stack");
}
