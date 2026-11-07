//! Expressions: Godot's Pratt parser (`parse_precedence` and the `get_rule`
//! table), types and lambdas.

use super::{Parser, is_identifier, is_node_name, token_name};
use crate::SyntaxKind::{self, *};

/// Godot's `Precedence`, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Precedence {
    None,
    Assignment,
    Cast,
    Ternary,
    LogicOr,
    LogicAnd,
    LogicNot,
    ContentTest,
    Comparison,
    BitOr,
    BitXor,
    BitAnd,
    BitShift,
    AdditionSubtraction,
    Factor,
    Sign,
    BitNot,
    Power,
    TypeTest,
    Await,
    Call,
    Attribute,
    Subscript,
    Primary,
}

impl Precedence {
    fn next(self) -> Precedence {
        use Precedence::*;
        const ORDER: [Precedence; 24] = [
            None,
            Assignment,
            Cast,
            Ternary,
            LogicOr,
            LogicAnd,
            LogicNot,
            ContentTest,
            Comparison,
            BitOr,
            BitXor,
            BitAnd,
            BitShift,
            AdditionSubtraction,
            Factor,
            Sign,
            BitNot,
            Power,
            TypeTest,
            Await,
            Call,
            Attribute,
            Subscript,
            Primary,
        ];
        ORDER[(self as usize + 1).min(ORDER.len() - 1)]
    }
}

/// The infix column of Godot's `get_rule` table.
fn infix_precedence(kind: SyntaxKind) -> Precedence {
    use Precedence as P;
    match kind {
        LESS | LESS_EQUAL | GREATER | GREATER_EQUAL | EQUAL_EQUAL | BANG_EQUAL => P::Comparison,
        AND_KW | AMPERSAND_AMPERSAND => P::LogicAnd,
        OR_KW | PIPE_PIPE => P::LogicOr,
        NOT_KW | IN_KW => P::ContentTest,
        AMPERSAND => P::BitAnd,
        PIPE => P::BitOr,
        CARET => P::BitXor,
        LESS_LESS | GREATER_GREATER => P::BitShift,
        PLUS | MINUS => P::AdditionSubtraction,
        STAR | SLASH | PERCENT => P::Factor,
        STAR_STAR => P::Power,
        EQUAL
        | PLUS_EQUAL
        | MINUS_EQUAL
        | STAR_EQUAL
        | STAR_STAR_EQUAL
        | SLASH_EQUAL
        | PERCENT_EQUAL
        | LESS_LESS_EQUAL
        | GREATER_GREATER_EQUAL
        | AMPERSAND_EQUAL
        | PIPE_EQUAL
        | CARET_EQUAL => P::Assignment,
        IF_KW => P::Ternary,
        AS_KW | QUESTION_MARK => P::Cast,
        IS_KW => P::TypeTest,
        BRACKET_OPEN => P::Subscript,
        PARENTHESIS_OPEN => P::Call,
        PERIOD => P::Attribute,
        _ => P::None,
    }
}

/// Whether the prefix column of Godot's `get_rule` table has a rule.
fn has_prefix(kind: SyntaxKind) -> bool {
    kind.is_literal()
        || matches!(
            kind,
            IDENTIFIER
                | NOT_KW
                | BANG
                | TILDE
                | PLUS
                | MINUS
                | PERCENT
                | AWAIT_KW
                | FUNC_KW
                | PRELOAD_KW
                | SELF_KW
                | SUPER_KW
                | YIELD_KW
                | BRACKET_OPEN
                | BRACE_OPEN
                | PARENTHESIS_OPEN
                | DOLLAR
        )
}

/// What an expression turned out to be, as Godot's node type: a grouping
/// `(x)` counts as `x`, as in Godot's tree.
#[derive(Debug, Clone, Copy)]
pub(super) struct Expression {
    pub kind: SyntaxKind,
    /// For a `LITERAL`, the kind of its token.
    pub literal: Option<SyntaxKind>,
}

impl Expression {
    fn node(kind: SyntaxKind) -> Self {
        Expression {
            kind,
            literal: None,
        }
    }
}

/// What `parse_type` read.
pub(super) struct TypeInfo {
    has_container: bool,
}

impl Parser<'_> {
    /// Godot's `parse_expression`.
    pub(super) fn parse_expression(&mut self, can_assign: bool) -> Option<Expression> {
        self.parse_precedence(Precedence::Assignment, can_assign, false)
    }

    /// Godot's `parse_precedence`.
    pub(super) fn parse_precedence(
        &mut self,
        precedence: Precedence,
        can_assign: bool,
        stop_on_assign: bool,
    ) -> Option<Expression> {
        if !self.enter() {
            return None;
        }
        let result = self.parse_precedence_inner(precedence, can_assign, stop_on_assign);
        self.leave();
        result
    }

    fn parse_precedence_inner(
        &mut self,
        precedence: Precedence,
        can_assign: bool,
        stop_on_assign: bool,
    ) -> Option<Expression> {
        // Turn multiline mode on early for brackets, so the tokenizer
        // doesn't produce whitespace tokens.
        if matches!(self.current, PARENTHESIS_OPEN | BRACE_OPEN | BRACKET_OPEN) {
            self.push_multiline(true);
        }
        let token = if is_identifier(self.current) {
            IDENTIFIER
        } else {
            self.current
        };
        if !has_prefix(token) {
            // The caller reports the missing expression.
            return None;
        }
        let checkpoint = self.checkpoint();
        let mut operand = self.parse_prefix(token, checkpoint, can_assign);

        // Each operator wraps the expression so far one level deeper in the
        // tree, so a long chain like `a.b.c...` counts toward the limit too.
        let mut chain = 0;
        while precedence <= infix_precedence(self.current) {
            let left = operand?;
            if (stop_on_assign && self.current == EQUAL) || self.lambda_ended {
                return Some(left);
            }
            chain += 1;
            if self.depth + chain >= super::MAX_DEPTH {
                self.report_too_deep();
                return Some(left);
            }
            let op = self.current;
            if matches!(op, PARENTHESIS_OPEN | BRACKET_OPEN) {
                self.push_multiline(true);
            }
            operand = self.parse_infix(op, left, checkpoint, can_assign);
        }
        operand
    }

    /// The prefix rules of Godot's `get_rule`, for the current token.
    fn parse_prefix(
        &mut self,
        token: SyntaxKind,
        checkpoint: rowan::Checkpoint,
        can_assign: bool,
    ) -> Option<Expression> {
        let _ = can_assign;
        let node = |p: &mut Self, kind: SyntaxKind| {
            p.start_at(checkpoint, kind);
            p.advance();
        };
        match token {
            IDENTIFIER => {
                node(self, NAME_REF);
                self.finish_node();
                Some(Expression::node(NAME_REF))
            }
            kind if kind.is_literal() => {
                node(self, LITERAL);
                self.finish_node();
                Some(Expression {
                    kind: LITERAL,
                    literal: Some(kind),
                })
            }
            SELF_KW => {
                node(self, SELF_EXPR);
                if self.function.is_some_and(|f| f.is_static) {
                    self.push_error("Cannot use \"self\" inside a static function.");
                }
                self.finish_node();
                Some(Expression::node(SELF_EXPR))
            }
            NOT_KW | BANG | TILDE | PLUS | MINUS => {
                node(self, UNARY_EXPR);
                let (operand_precedence, symbol) = match token {
                    MINUS => (Precedence::Sign, "-"),
                    PLUS => (Precedence::Sign, "+"),
                    TILDE => (Precedence::BitNot, "~"),
                    NOT_KW => (Precedence::LogicNot, "not"),
                    _ => (Precedence::LogicNot, "!"),
                };
                if self
                    .parse_precedence(operand_precedence, false, false)
                    .is_none()
                {
                    self.push_error(format!("Expected expression after \"{symbol}\" operator."));
                }
                self.finish_node();
                Some(Expression::node(UNARY_EXPR))
            }
            DOLLAR | PERCENT => {
                node(self, GET_NODE);
                let result = self.parse_get_node();
                self.finish_node();
                result.then_some(Expression::node(GET_NODE))
            }
            AWAIT_KW => {
                node(self, AWAIT_EXPR);
                if self
                    .parse_precedence(Precedence::Await, false, false)
                    .is_none()
                {
                    self.push_error("Expected signal or coroutine after \"await\".");
                }
                self.finish_node();
                Some(Expression::node(AWAIT_EXPR))
            }
            BRACKET_OPEN => {
                node(self, ARRAY);
                self.parse_array();
                self.finish_node();
                Some(Expression::node(ARRAY))
            }
            BRACE_OPEN => {
                node(self, DICT);
                self.parse_dictionary();
                self.finish_node();
                Some(Expression::node(DICT))
            }
            PARENTHESIS_OPEN => {
                node(self, PAREN_EXPR);
                let inner = self.parse_expression(false);
                self.pop_multiline();
                match inner {
                    None => self.push_error("Expected grouping expression."),
                    Some(_) => {
                        self.consume(
                            PARENTHESIS_CLOSE,
                            "Expected closing \")\" after grouping expression.",
                        );
                    }
                }
                self.finish_node();
                inner
            }
            PRELOAD_KW => {
                node(self, PRELOAD);
                self.push_multiline(true);
                self.start(ARG_LIST);
                self.consume(PARENTHESIS_OPEN, "Expected \"(\" after \"preload\".");
                if self.parse_expression(false).is_none() {
                    self.push_error("Expected resource path after \"(\".");
                }
                self.eat(COMMA); // Trailing comma.
                self.pop_multiline();
                self.consume(PARENTHESIS_CLOSE, "Expected \")\" after preload path.");
                self.finish_node();
                self.finish_node();
                Some(Expression::node(PRELOAD))
            }
            SUPER_KW => {
                node(self, CALL);
                let result = self.parse_super_call();
                self.finish_node();
                result.then_some(Expression::node(CALL))
            }
            YIELD_KW => {
                node(self, ERROR);
                self.push_error("\"yield\" was removed in Godot 4. Use \"await\" instead.");
                self.finish_node();
                None
            }
            FUNC_KW => {
                node(self, LAMBDA);
                self.parse_lambda();
                self.finish_node();
                Some(Expression::node(LAMBDA))
            }
            _ => None,
        }
    }

    /// The infix rules of Godot's `get_rule`, with the operator current.
    fn parse_infix(
        &mut self,
        op: SyntaxKind,
        left: Expression,
        checkpoint: rowan::Checkpoint,
        can_assign: bool,
    ) -> Option<Expression> {
        match op {
            PARENTHESIS_OPEN => {
                self.start_at(checkpoint, CALL);
                if !matches!(left.kind, NAME_REF | ATTRIBUTE) {
                    let (start, end) = (self.current_start, self.current_end);
                    self.push_error_at(
                        "Cannot call on an expression. Use \".call()\" if it's a Callable.",
                        start,
                        end,
                    );
                }
                self.start(ARG_LIST);
                self.advance();
                self.parse_call_arguments();
                self.finish_node();
                self.finish_node();
                Some(Expression::node(CALL))
            }
            BRACKET_OPEN => {
                self.start_at(checkpoint, SUBSCRIPT);
                self.advance();
                if self.parse_expression(false).is_none() {
                    self.push_error("Expected expression after \"[\".");
                }
                self.pop_multiline();
                self.consume(BRACKET_CLOSE, "Expected \"]\" after subscription index.");
                self.finish_node();
                Some(Expression::node(SUBSCRIPT))
            }
            PERIOD => {
                self.start_at(checkpoint, ATTRIBUTE);
                self.advance();
                if is_node_name(self.current) || is_identifier(self.current) {
                    self.advance();
                } else {
                    self.push_error("Expected identifier after \".\" for attribute access.");
                }
                self.finish_node();
                Some(Expression::node(ATTRIBUTE))
            }
            IF_KW => {
                self.start_at(checkpoint, TERNARY_EXPR);
                self.advance();
                if self
                    .parse_precedence(Precedence::Ternary, false, false)
                    .is_none()
                {
                    self.push_error("Expected expression as ternary condition after \"if\".");
                }
                self.consume(
                    ELSE_KW,
                    "Expected \"else\" after ternary operator condition.",
                );
                if self
                    .parse_precedence(Precedence::Ternary, false, false)
                    .is_none()
                {
                    self.push_error("Expected expression after \"else\".");
                }
                self.finish_node();
                Some(Expression::node(TERNARY_EXPR))
            }
            AS_KW => {
                self.start_at(checkpoint, CAST);
                self.advance();
                let parsed = self.parse_type(false).is_some();
                if !parsed {
                    self.push_error("Expected type specifier after \"as\".");
                }
                self.finish_node();
                Some(if parsed { Expression::node(CAST) } else { left })
            }
            IS_KW => {
                self.start_at(checkpoint, TYPE_TEST);
                self.advance();
                let negated = self.eat(NOT_KW);
                if self.parse_type(false).is_none() {
                    self.push_error(if negated {
                        "Expected type specifier after \"is not\"."
                    } else {
                        "Expected type specifier after \"is\"."
                    });
                }
                self.finish_node();
                Some(Expression::node(if negated {
                    UNARY_EXPR
                } else {
                    TYPE_TEST
                }))
            }
            QUESTION_MARK => {
                self.start_at(checkpoint, ERROR);
                self.advance();
                self.push_error(
                    "Unexpected \"?\" in source. If you want a ternary operator, use \"truthy_value if true_condition else falsy_value\".",
                );
                self.finish_node();
                Some(left)
            }
            NOT_KW => {
                // `not in`.
                self.start_at(checkpoint, BINARY_EXPR);
                self.advance();
                self.consume(
                    IN_KW,
                    "Expected \"in\" after \"not\" in content-test operator.",
                );
                self.parse_binary_rhs(IN_KW);
                self.finish_node();
                Some(Expression::node(UNARY_EXPR))
            }
            kind if infix_precedence(kind) == Precedence::Assignment => {
                self.start_at(checkpoint, ASSIGNMENT);
                self.advance();
                let result = self.parse_assignment(left, can_assign);
                self.finish_node();
                result
            }
            _ => {
                self.start_at(checkpoint, BINARY_EXPR);
                self.advance();
                self.parse_binary_rhs(op);
                self.finish_node();
                Some(Expression::node(BINARY_EXPR))
            }
        }
    }

    /// Godot's `parse_binary_operator`, after the operator.
    fn parse_binary_rhs(&mut self, op: SyntaxKind) {
        let precedence = infix_precedence(op).next();
        if self.parse_precedence(precedence, false, false).is_none() {
            self.push_error(format!(
                "Expected expression after \"{}\" operator.",
                token_name(op)
            ));
        }
    }

    /// Godot's `parse_assignment`, after the operator.
    fn parse_assignment(&mut self, target: Expression, can_assign: bool) -> Option<Expression> {
        if !can_assign {
            self.push_error("Assignment is not allowed inside an expression.");
            return self.parse_expression(false);
        }
        if !matches!(target.kind, NAME_REF | ATTRIBUTE | SUBSCRIPT) {
            self.push_error(
                "Only identifier, attribute access, and subscription access can be used as assignment target.",
            );
            return self.parse_expression(false);
        }
        if self.parse_expression(false).is_none() {
            self.push_error("Expected an expression after \"=\".");
        }
        Some(Expression::node(ASSIGNMENT))
    }

    /// The arguments of a call, after `(`, up to and including `)`.
    fn parse_call_arguments(&mut self) {
        loop {
            if self.check(PARENTHESIS_CLOSE) {
                break; // Trailing comma.
            }
            if self.parse_expression(false).is_none() {
                self.push_error("Expected expression as the function argument.");
            }
            if !self.eat(COMMA) {
                break;
            }
        }
        self.pop_multiline();
        self.consume(
            PARENTHESIS_CLOSE,
            "Expected closing \")\" after call arguments.",
        );
    }

    /// Godot's `parse_call` for `super`, after `super`.
    fn parse_super_call(&mut self) -> bool {
        if self.check(PARENTHESIS_OPEN) {
            self.push_multiline(true);
            self.start(ARG_LIST);
            self.advance();
            if self.function.is_none() {
                self.push_error("Cannot use implicit \"super\" call outside of a function.");
                self.pop_multiline();
                self.finish_node();
                return false;
            }
        } else {
            self.consume(PERIOD, "Expected \".\" or \"(\" after \"super\".");
            if !self.consume(IDENTIFIER, "Expected function name after \".\".") {
                return false;
            }
            if !self.check(PARENTHESIS_OPEN) {
                self.push_error("Expected \"(\" after function name.");
                return false;
            }
            self.push_multiline(true);
            self.start(ARG_LIST);
            self.advance();
        }
        self.parse_call_arguments();
        self.finish_node();
        true
    }

    /// Godot's `parse_array`, after `[`.
    fn parse_array(&mut self) {
        if !self.check(BRACKET_CLOSE) {
            loop {
                if self.check(BRACKET_CLOSE) {
                    break; // Trailing comma.
                }
                if self.parse_expression(false).is_none() {
                    self.push_error("Expected expression as array element.");
                }
                if !self.eat(COMMA) || self.is_at_end() {
                    break;
                }
            }
        }
        self.pop_multiline();
        self.consume(
            BRACKET_CLOSE,
            "Expected closing \"]\" after array elements.",
        );
    }

    /// Godot's `parse_dictionary`, after `{`. Supports both `{key: value}`
    /// and Lua-style `{key = value}`.
    fn parse_dictionary(&mut self) {
        #[derive(PartialEq)]
        enum Style {
            Undecided,
            Python,
            Lua,
        }
        let mut style = Style::Undecided;
        if !self.check(BRACE_CLOSE) {
            loop {
                if self.check(BRACE_CLOSE) {
                    break; // Trailing comma.
                }
                self.start(DICT_ENTRY);
                // Stop on `=` to recognize Lua style.
                let key = self.parse_precedence(Precedence::Assignment, false, true);
                if key.is_none() {
                    self.push_error("Expected expression as dictionary key.");
                }
                if style == Style::Undecided {
                    match self.current {
                        COLON => style = Style::Python,
                        EQUAL => style = Style::Lua,
                        _ => self.push_error("Expected \":\" or \"=\" after dictionary key."),
                    }
                    if style == Style::Undecided {
                        // Godot decides anyway, keeping its default.
                        style = Style::Python;
                    }
                }
                match style {
                    Style::Lua => {
                        if let Some(key) = key {
                            let valid = match key.kind {
                                NAME_REF => true,
                                LITERAL => key.literal == Some(STRING),
                                _ => false,
                            };
                            if !valid {
                                self.push_error("Expected identifier or string as Lua-style dictionary key (e.g \"{ key = value }\").");
                            }
                        }
                        if !self.eat(EQUAL) {
                            if self.eat(COLON) {
                                self.push_error("Expected \"=\" after dictionary key. Mixing dictionary styles is not allowed.");
                                self.advance(); // Consume the wrong separator anyway.
                            } else {
                                self.push_error("Expected \"=\" after dictionary key.");
                            }
                        }
                    }
                    _ => {
                        if !self.eat(COLON) {
                            if self.eat(EQUAL) {
                                self.push_error("Expected \":\" after dictionary key. Mixing dictionary styles is not allowed.");
                                self.advance(); // Consume the wrong separator anyway.
                            } else {
                                self.push_error("Expected \":\" after dictionary key.");
                            }
                        }
                    }
                }
                if self.parse_expression(false).is_none() {
                    self.push_error("Expected expression as dictionary value.");
                }
                self.finish_node();
                if !self.eat(COMMA) || self.is_at_end() {
                    break;
                }
            }
        }
        self.pop_multiline();
        self.consume(
            BRACE_CLOSE,
            "Expected closing \"}\" after dictionary elements.",
        );
    }

    /// Godot's `parse_get_node`, after `$` or `%`.
    fn parse_get_node(&mut self) -> bool {
        let start_token = if self.previous == DOLLAR { "$" } else { "%" };
        if !is_node_name(self.current)
            && !self.check_literal()
            && !self.check(SLASH)
            && !self.check(PERCENT)
        {
            self.push_error(format!(
                "Expected node path as string or identifier after \"{start_token}\"."
            ));
            return false;
        }
        if self.check_literal() && self.current != STRING {
            self.push_error(format!(
                "Expected node path as string or identifier after \"{start_token}\"."
            ));
            return false;
        }

        #[derive(PartialEq)]
        enum PathState {
            Start,
            Slash,
            Percent,
            NodeName,
        }
        let mut state = PathState::Start;
        if self.previous == DOLLAR {
            // A leading slash, handled in the loop.
            self.eat(SLASH);
        }
        loop {
            if self.previous == PERCENT {
                if state != PathState::Start && state != PathState::Slash {
                    self.push_error(
                        "\"%\" is only valid in the beginning of a node name (either after \"$\" or after \"/\")",
                    );
                    return false;
                }
                state = PathState::Percent;
            } else if self.previous == SLASH {
                if state != PathState::Start && state != PathState::NodeName {
                    self.push_error(
                        "\"/\" is only valid at the beginning of the path or after a node name.",
                    );
                    return false;
                }
                state = PathState::Slash;
            }

            if self.check_literal() {
                self.advance();
                if self.previous != STRING {
                    let after = match state {
                        PathState::Start => "$",
                        PathState::Percent => "%",
                        PathState::Slash => "/",
                        PathState::NodeName => "",
                    };
                    self.push_error(format!(
                        "Expected node path as string or identifier after \"{after}\"."
                    ));
                    return false;
                }
                state = PathState::NodeName;
            } else if is_node_name(self.current) {
                self.advance();
                state = PathState::NodeName;
            } else if !self.check(SLASH) && !self.check(PERCENT) {
                let name = token_name(self.current);
                let (start, end) = (self.previous_start, self.previous_end);
                self.push_error_at(format!("Unexpected \"{name}\" in node path."), start, end);
                return false;
            }

            if !(self.eat(SLASH) || self.eat(PERCENT)) {
                break;
            }
        }
        true
    }

    /// Godot's `parse_lambda`, after `func`.
    fn parse_lambda(&mut self) {
        let named = self.eat(IDENTIFIER);
        let multiline_context = self.multiline_stack.last().copied().unwrap_or(false);

        // The lambda body is not in multiline mode.
        self.push_multiline(false);
        if multiline_context {
            self.lexer.push_expression_indented_block();
        }

        self.push_multiline(true); // For the parameters.
        self.start(PARAM_LIST);
        self.consume(
            PARENTHESIS_OPEN,
            if named {
                "Expected opening \"(\" after lambda name."
            } else {
                "Expected opening \"(\" after \"func\"."
            },
        );

        let is_static = self.function.is_some_and(|f| f.is_static);
        let previous_in_lambda = self.in_lambda;
        let (could_break, could_continue) = (self.can_break, self.can_continue);
        self.in_function(is_static, false, |p| {
            p.parse_function_signature("lambda", None);
            p.in_lambda = true;
            p.can_break = false;
            p.can_continue = false;
            p.parse_suite("lambda declaration", true);
        });

        self.pop_multiline();
        if multiline_context {
            // Skip the dedents and line breaks the body left behind.
            while matches!(self.current, DEDENT | INDENT | NEWLINE) {
                self.demote_current();
            }
            self.lexer.pop_expression_indented_block();
        }
        self.in_lambda = previous_in_lambda;
        self.can_break = could_break;
        self.can_continue = could_continue;
    }

    /// Godot's `parse_type`. Returns `None` without an error when there is
    /// no type: the caller knows what to say.
    pub(super) fn parse_type(&mut self, allow_void: bool) -> Option<TypeInfo> {
        if !self.check(IDENTIFIER) {
            if self.check(VOID_KW) {
                if allow_void {
                    self.start(TYPE);
                    self.advance();
                    self.finish_node();
                    return Some(TypeInfo {
                        has_container: false,
                    });
                }
                self.advance();
                self.push_error("\"void\" is only allowed for a function return type.");
            }
            return None;
        }
        if !self.enter() {
            return None;
        }
        self.start(TYPE);
        self.advance();

        if self.check(BRACKET_OPEN) {
            // A typed collection, like `Array[int]` or `Dictionary[String, int]`.
            self.start(TYPE_ARGS);
            self.advance();
            let mut valid = true;
            let mut first = true;
            loop {
                match self.parse_type(false) {
                    None => {
                        self.push_error(format!(
                            "Expected type for collection after \"{}\".",
                            if first { "[" } else { "," }
                        ));
                        valid = false;
                        break;
                    }
                    Some(inner) if inner.has_container => {
                        self.push_error("Nested typed collections are not supported.");
                    }
                    Some(_) => {}
                }
                first = false;
                if !self.eat(COMMA) {
                    break;
                }
            }
            self.consume(
                BRACKET_CLOSE,
                "Expected closing \"]\" after collection type.",
            );
            self.finish_node();
            self.finish_node();
            self.leave();
            return valid.then_some(TypeInfo {
                has_container: true,
            });
        }

        while self.eat(PERIOD) {
            self.consume(IDENTIFIER, "Expected inner type name after \".\".");
        }
        self.finish_node();
        self.leave();
        Some(TypeInfo {
            has_container: false,
        })
    }
}
