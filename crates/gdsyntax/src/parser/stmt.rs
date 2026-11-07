//! Statements: Godot's `parse_suite`, `parse_statement` and the statements
//! it reads, including `match` and its patterns.

use super::{Parser, target, token_name};
use crate::SyntaxKind::{self, *};

/// What `parse_match_pattern` read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pattern {
    Bind,
    Wildcard,
    Rest,
    Other,
}

struct PatternInfo {
    kind: Pattern,
    has_bind: bool,
}

impl Parser<'_> {
    /// Godot's `parse_suite`: a block, inline or indented.
    pub(super) fn parse_suite(&mut self, context: &str, for_lambda: bool) {
        self.start(SUITE);
        let multiline = self.eat(NEWLINE);
        if multiline && !self.consume(INDENT, &format!("Expected indented block after {context}."))
        {
            self.finish_node();
            return;
        }

        let mut error_count = 0;
        loop {
            if self.is_at_end() || (!multiline && self.previous == SEMICOLON && self.check(NEWLINE))
            {
                break;
            }
            if !self.parse_statement() {
                error_count += 1;
                if error_count > 100 {
                    self.push_error("Too many statement errors.");
                    break;
                }
            }
            let more = (multiline || self.previous == SEMICOLON)
                && !self.check(DEDENT)
                && !self.lambda_ended
                && !self.is_at_end();
            if !more {
                break;
            }
        }

        if multiline {
            if !self.lambda_ended {
                self.consume(
                    DEDENT,
                    &format!("Missing unindent at the end of {context}."),
                );
            } else {
                self.eat(DEDENT);
            }
        } else if self.previous == SEMICOLON {
            self.consume(
                NEWLINE,
                &format!("Expected newline after \";\" at the end of {context}."),
            );
        }
        if for_lambda {
            self.lambda_ended = true;
        }
        self.finish_node();
    }

    /// Godot's `parse_statement`. Returns false where Godot returns no node.
    fn parse_statement(&mut self) -> bool {
        if !self.enter() {
            // Too deep: skip the rest of the line.
            self.start(ERROR);
            while !self.is_statement_end_token() {
                self.advance();
            }
            self.eat(NEWLINE);
            self.finish_node();
            return false;
        }
        let result = self.parse_statement_inner();
        self.leave();
        if self.panic_mode {
            self.synchronize();
        }
        result
    }

    fn parse_statement_inner(&mut self) -> bool {
        let checkpoint = {
            let parent = self.node_id();
            let first = self
                .annotation_stack
                .first()
                .filter(|a| a.parent == parent)
                .map(|a| a.checkpoint);
            match first {
                Some(checkpoint) if self.current != ANNOTATION => checkpoint,
                _ => self.checkpoint(),
            }
        };
        if self.current != ANNOTATION {
            self.attach_annotations(target::STATEMENT, "a statement");
        }

        let simple = |p: &mut Self, kind: SyntaxKind, context: &str| {
            p.start_at(checkpoint, kind);
            p.advance();
            p.end_statement(context);
            p.finish_node();
            true
        };

        match self.current {
            PASS_KW => simple(self, PASS_STMT, "\"pass\""),
            BREAKPOINT_KW => simple(self, BREAKPOINT_STMT, "\"breakpoint\""),
            VAR_KW => {
                self.start_at(checkpoint, VAR);
                self.advance();
                self.parse_variable(false, false);
                self.finish_node();
                true
            }
            CONST_KW => {
                self.start_at(checkpoint, CONST);
                self.advance();
                self.parse_constant();
                self.finish_node();
                true
            }
            IF_KW => {
                self.start_at(checkpoint, IF_STMT);
                self.advance();
                self.parse_if();
                self.finish_node();
                true
            }
            FOR_KW => {
                self.start_at(checkpoint, FOR_STMT);
                self.advance();
                self.parse_for();
                self.finish_node();
                true
            }
            WHILE_KW => {
                self.start_at(checkpoint, WHILE_STMT);
                self.advance();
                self.parse_while();
                self.finish_node();
                true
            }
            MATCH_KW => {
                self.start_at(checkpoint, MATCH_STMT);
                self.advance();
                self.parse_match();
                self.finish_node();
                true
            }
            BREAK_KW | CONTINUE_KW => {
                let is_break = self.current == BREAK_KW;
                self.start_at(
                    checkpoint,
                    if is_break { BREAK_STMT } else { CONTINUE_STMT },
                );
                self.advance();
                if is_break && !self.can_break {
                    self.push_error("Cannot use \"break\" outside of a loop.");
                }
                if !is_break && !self.can_continue {
                    self.push_error("Cannot use \"continue\" outside of a loop.");
                }
                self.end_statement(if is_break {
                    "\"break\""
                } else {
                    "\"continue\""
                });
                self.finish_node();
                true
            }
            RETURN_KW => {
                self.start_at(checkpoint, RETURN_STMT);
                self.advance();
                if !self.is_statement_end() {
                    if self.function.is_some_and(|f| f.is_constructor) {
                        self.push_error("Constructor cannot return a value.");
                    }
                    self.parse_expression(false);
                } else if self.in_lambda && !self.is_statement_end_token() {
                    // Maybe not the end of the statement inside a lambda.
                    self.parse_expression(false);
                }
                self.end_statement("return statement");
                self.finish_node();
                true
            }
            ASSERT_KW => {
                self.start_at(checkpoint, ASSERT_STMT);
                self.advance();
                let result = self.parse_assert();
                self.finish_node();
                result
            }
            ANNOTATION => {
                if let Some(annotation) =
                    self.parse_annotation(target::STATEMENT | target::STANDALONE)
                {
                    if annotation.targets & target::STANDALONE != 0 {
                        if self.previous != NEWLINE {
                            self.push_error("Expected newline after a standalone annotation.");
                        }
                        if !matches!(
                            annotation.name.as_str(),
                            "@warning_ignore_start" | "@warning_ignore_restore"
                        ) {
                            self.push_error("Unexpected standalone annotation.");
                        }
                    } else {
                        self.annotation_stack.push(annotation);
                    }
                }
                false
            }
            _ => {
                // Expression statement.
                let start = self.current_start;
                match self.parse_expression(true) {
                    None if self.in_lambda => {
                        // Perhaps the rest of the expression around the lambda.
                        self.lambda_ended = true;
                        false
                    }
                    None => {
                        self.start_at(checkpoint, ERROR);
                        self.advance();
                        let name = token_name(self.previous);
                        self.push_error(format!("Expected statement, found \"{name}\" instead."));
                        self.finish_node();
                        false
                    }
                    Some(expression) => {
                        self.start_at(checkpoint, EXPR_STMT);
                        if expression.kind == LAMBDA {
                            let end = self.previous_end;
                            self.push_error_at(
                                "Standalone lambdas cannot be accessed. Consider assigning it to a variable.",
                                start,
                                end,
                            );
                        }
                        self.end_statement("expression");
                        self.finish_node();
                        true
                    }
                }
            }
        }
    }

    /// Godot's `parse_assert`, after `assert`.
    fn parse_assert(&mut self) -> bool {
        self.push_multiline(true);
        self.start(ARG_LIST);
        self.consume(PARENTHESIS_OPEN, "Expected \"(\" after \"assert\".");
        if self.parse_expression(false).is_none() {
            self.push_error("Expected expression to assert.");
            self.pop_multiline();
            self.finish_node();
            return false;
        }
        if self.eat(COMMA) && !self.check(PARENTHESIS_CLOSE) {
            if self.parse_expression(false).is_none() {
                self.push_error("Expected error message for assert after \",\".");
                self.pop_multiline();
                self.finish_node();
                return false;
            }
            self.eat(COMMA);
        }
        self.pop_multiline();
        self.consume(PARENTHESIS_CLOSE, "Expected \")\" after assert expression.");
        self.finish_node();
        self.end_statement("\"assert\"");
        true
    }

    /// Run `f` inside a loop body, where `break` and `continue` are allowed.
    fn in_loop(&mut self, f: impl FnOnce(&mut Self)) {
        let (could_break, could_continue) = (self.can_break, self.can_continue);
        self.can_break = true;
        self.can_continue = true;
        f(self);
        self.can_break = could_break;
        self.can_continue = could_continue;
    }

    /// Godot's `parse_for`, after `for`.
    fn parse_for(&mut self) {
        self.consume(IDENTIFIER, "Expected loop variable name after \"for\".");
        let mut typed = false;
        if self.eat(COLON) {
            if self.parse_type(false).is_none() {
                self.push_error("Expected type specifier after \":\".");
            } else {
                typed = true;
            }
        }
        if typed {
            self.consume(
                IN_KW,
                "Expected \"in\" after \"for\" variable type specifier.",
            );
        } else {
            self.consume(
                IN_KW,
                "Expected \"in\" or \":\" after \"for\" variable name.",
            );
        }
        if self.parse_expression(false).is_none() {
            self.push_error("Expected iterable after \"in\".");
        }
        self.consume(COLON, "Expected \":\" after \"for\" condition.");
        self.in_loop(|p| p.parse_suite("\"for\" block", false));
    }

    /// Godot's `parse_if`, after `if`, with the `elif` and `else` that follow.
    fn parse_if(&mut self) {
        let mut keyword = "if";
        loop {
            if self.parse_expression(false).is_none() {
                self.push_error(format!(
                    "Expected conditional expression after \"{keyword}\"."
                ));
            }
            self.consume(
                COLON,
                &format!("Expected \":\" after \"{keyword}\" condition."),
            );
            self.parse_suite(&format!("\"{keyword}\" block"), false);
            if keyword == "elif" {
                self.finish_node(); // ELIF_CLAUSE
            }
            if self.check(ELIF_KW) {
                self.start(ELIF_CLAUSE);
                self.advance();
                keyword = "elif";
                continue;
            }
            if self.check(ELSE_KW) {
                self.start(ELSE_CLAUSE);
                self.advance();
                self.consume(COLON, "Expected \":\" after \"else\".");
                self.parse_suite("\"else\" block", false);
                self.finish_node();
            }
            return;
        }
    }

    /// Godot's `parse_while`, after `while`.
    fn parse_while(&mut self) {
        if self.parse_expression(false).is_none() {
            self.push_error("Expected conditional expression after \"while\".");
        }
        self.consume(COLON, "Expected \":\" after \"while\" condition.");
        self.in_loop(|p| p.parse_suite("\"while\" block", false));
    }

    /// Godot's `parse_match`, after `match`.
    fn parse_match(&mut self) {
        if self.parse_expression(false).is_none() {
            self.push_error("Expected expression to test after \"match\".");
        }
        self.consume(COLON, "Expected \":\" after \"match\" expression.");
        self.consume(NEWLINE, "Expected a newline after \"match\" statement.");
        if !self.consume(
            INDENT,
            "Expected an indented block after \"match\" statement.",
        ) {
            return;
        }

        let mut branch_annotations = Vec::new();
        while !self.check(DEDENT) && !self.is_at_end() {
            if self.check(PASS_KW) {
                self.start(PASS_STMT);
                self.advance();
                self.consume(NEWLINE, "Expected newline after \"pass\".");
                self.finish_node();
                continue;
            }
            if self.check(ANNOTATION) {
                let Some(annotation) = self.parse_annotation(target::STATEMENT) else {
                    continue;
                };
                if annotation.name != "@warning_ignore" {
                    self.push_error_at(
                        format!(
                            "Annotation \"{}\" is not allowed in this level.",
                            annotation.name
                        ),
                        annotation.start,
                        annotation.end,
                    );
                    continue;
                }
                branch_annotations.push(annotation);
                continue;
            }
            let parent = self.node_id();
            let checkpoint = match branch_annotations.first() {
                Some(a) if a.parent == parent => a.checkpoint,
                _ => self.checkpoint(),
            };
            branch_annotations.clear();
            self.parse_match_branch(checkpoint);
        }
        self.consume(
            DEDENT,
            "Expected an indented block after \"match\" statement.",
        );
        for annotation in branch_annotations {
            self.push_error_at(
                format!(
                    "Annotation \"{}\" does not precede a valid target, so it will have no effect.",
                    annotation.name
                ),
                annotation.start,
                annotation.end,
            );
        }
    }

    /// Godot's `parse_match_branch`.
    fn parse_match_branch(&mut self, checkpoint: rowan::Checkpoint) {
        self.start_at(checkpoint, MATCH_BRANCH);
        let mut pattern_count = 0;
        let mut has_bind = false;
        loop {
            if let Some(pattern) = self.parse_match_pattern() {
                has_bind |= pattern.has_bind;
                if pattern_count > 0 && has_bind {
                    self.push_error("Cannot use a variable bind with multiple patterns.");
                }
                if pattern.kind == Pattern::Rest {
                    self.push_error(
                        "Rest pattern can only be used inside array and dictionary patterns.",
                    );
                }
                pattern_count += 1;
            }
            if !self.eat(COMMA) {
                break;
            }
        }
        if pattern_count == 0 {
            self.push_error("No pattern found for \"match\" branch.");
        }

        let mut has_guard = false;
        if self.check(WHEN_KW) {
            self.start(PATTERN_GUARD);
            self.advance();
            if self.parse_expression(false).is_none() {
                self.push_error("Expected expression for pattern guard after \"when\".");
            }
            self.finish_node();
            has_guard = true;
        }

        let message = if has_guard {
            "Expected \":\" after \"match\" pattern guard."
        } else {
            "Expected \":\" or \"when\" after \"match\" patterns."
        };
        if !self.consume(COLON, message) {
            // Skip the line and treat the next one as a new branch.
            while self.current != NEWLINE && !self.is_at_end() {
                self.advance();
            }
            if !self.is_at_end() {
                self.advance();
            }
            self.finish_node();
            return;
        }
        self.parse_suite("match pattern block", false);
        self.finish_node();
    }

    /// Godot's `parse_match_pattern`.
    fn parse_match_pattern(&mut self) -> Option<PatternInfo> {
        if !self.enter() {
            return None;
        }
        let result = self.parse_match_pattern_inner();
        self.leave();
        result
    }

    fn parse_match_pattern_inner(&mut self) -> Option<PatternInfo> {
        let simple = |kind| PatternInfo {
            kind,
            has_bind: false,
        };
        match self.current {
            VAR_KW => {
                self.start(BIND_PATTERN);
                self.advance();
                let ok = self.consume(IDENTIFIER, "Expected bind name after \"var\".");
                self.finish_node();
                ok.then_some(PatternInfo {
                    kind: Pattern::Bind,
                    has_bind: true,
                })
            }
            UNDERSCORE => {
                self.start(WILDCARD_PATTERN);
                self.advance();
                self.finish_node();
                Some(simple(Pattern::Wildcard))
            }
            PERIOD_PERIOD => {
                self.start(REST_PATTERN);
                self.advance();
                self.finish_node();
                Some(simple(Pattern::Rest))
            }
            BRACKET_OPEN => {
                self.start(ARRAY_PATTERN);
                self.push_multiline(true);
                self.advance();
                let (mut rest_used, mut has_bind) = (false, false);
                loop {
                    if self.is_at_end() || self.check(BRACKET_CLOSE) {
                        break;
                    }
                    if let Some(sub) = self.parse_match_pattern() {
                        if rest_used {
                            self.push_error(
                                "The \"..\" pattern must be the last element in the pattern array.",
                            );
                        } else if sub.kind == Pattern::Rest {
                            rest_used = true;
                        }
                        has_bind |= sub.has_bind;
                    }
                    if !self.eat(COMMA) {
                        break;
                    }
                }
                self.consume(BRACKET_CLOSE, "Expected \"]\" to close the array pattern.");
                self.pop_multiline();
                self.finish_node();
                Some(PatternInfo {
                    kind: Pattern::Other,
                    has_bind,
                })
            }
            BRACE_OPEN => {
                self.start(DICT_PATTERN);
                self.push_multiline(true);
                self.advance();
                let (mut rest_used, mut has_bind) = (false, false);
                loop {
                    if self.check(BRACE_CLOSE) || self.is_at_end() {
                        break;
                    }
                    if self.check(PERIOD_PERIOD) {
                        self.start(REST_PATTERN);
                        self.advance();
                        self.finish_node();
                        if rest_used {
                            self.push_error(
                                "The \"..\" pattern must be the last element in the pattern dictionary.",
                            );
                        } else {
                            rest_used = true;
                        }
                    } else {
                        self.start(DICT_PATTERN_ENTRY);
                        if self.parse_expression(false).is_none() {
                            self.push_error("Expected expression as key for dictionary pattern.");
                        }
                        if self.eat(COLON)
                            && let Some(sub) = self.parse_match_pattern()
                        {
                            if rest_used {
                                self.push_error(
                                    "The \"..\" pattern must be the last element in the pattern dictionary.",
                                );
                            } else if sub.kind == Pattern::Rest {
                                self.push_error("The \"..\" pattern cannot be used as a value.");
                            }
                            has_bind |= sub.has_bind;
                        }
                        self.finish_node();
                    }
                    if !self.eat(COMMA) {
                        break;
                    }
                }
                self.consume(
                    BRACE_CLOSE,
                    "Expected \"}\" to close the dictionary pattern.",
                );
                self.pop_multiline();
                self.finish_node();
                Some(PatternInfo {
                    kind: Pattern::Other,
                    has_bind,
                })
            }
            _ => {
                let checkpoint = self.checkpoint();
                if self.parse_expression(false).is_none() {
                    self.push_error("Expected expression for match pattern.");
                    return None;
                }
                self.start_at(checkpoint, EXPR_PATTERN);
                self.finish_node();
                Some(simple(Pattern::Other))
            }
        }
    }
}
