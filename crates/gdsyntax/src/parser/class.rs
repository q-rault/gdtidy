//! Class level: Godot's `parse_program`, `parse_class_body` and the
//! declarations they read.

use super::{Parser, PendingAnnotation, annotation_targets, target, token_name};
use crate::SyntaxKind::{self, *};
use rowan::Checkpoint;

/// What `parse_parameter` read.
pub(super) struct Parameter {
    pub has_default: bool,
}

impl Parser<'_> {
    /// Godot's `parse_program`: script annotations, `class_name` and
    /// `extends`, then the class body.
    pub(super) fn parse_program(&mut self) {
        let mut can_have_class_or_extends = true;
        let mut has_class_name = false;
        self.class_extends_used = false;

        while !self.is_at_end() {
            if self.check(ANNOTATION) {
                let Some(annotation) = self
                    .parse_annotation(target::SCRIPT | target::CLASS_LEVEL | target::STANDALONE)
                else {
                    continue;
                };
                if annotation.targets & target::CLASS != 0 {
                    // Not known yet: the script's class or an inner class.
                    self.annotation_stack.push(annotation);
                } else if annotation.targets & target::SCRIPT != 0 {
                    self.annotation_stack.clear(); // They belong to the script.
                } else if annotation.targets & target::STANDALONE != 0 {
                    if self.previous != NEWLINE {
                        self.push_error("Expected newline after a standalone annotation.");
                    }
                    if is_export_group(&annotation.name) {
                        can_have_class_or_extends = false;
                        break;
                    } else if !is_warning_region(&annotation.name) {
                        self.push_error("Unexpected standalone annotation.");
                    }
                } else {
                    self.annotation_stack.push(annotation);
                    can_have_class_or_extends = false;
                    break;
                }
            } else if self.current == STRING {
                self.parse_string_statement();
            } else {
                break;
            }
        }

        while can_have_class_or_extends {
            match self.current {
                CLASS_NAME_KW => {
                    self.annotation_stack.clear();
                    self.start(CLASS_NAME);
                    self.advance();
                    if has_class_name {
                        self.push_error("\"class_name\" can only be used once.");
                    } else {
                        has_class_name = true;
                        self.parse_class_name();
                    }
                    self.finish_node();
                }
                EXTENDS_KW => {
                    self.annotation_stack.clear();
                    self.start(EXTENDS);
                    self.advance();
                    if self.class_extends_used {
                        self.push_error("\"extends\" can only be used once.");
                    } else {
                        self.parse_extends();
                        self.end_statement("superclass");
                    }
                    self.finish_node();
                }
                EOF => {
                    self.annotation_stack.clear();
                    can_have_class_or_extends = false;
                }
                STRING => self.parse_string_statement(),
                _ => can_have_class_or_extends = false,
            }
            if self.panic_mode {
                self.synchronize();
            }
        }

        self.parse_class_body(true);

        if !self.is_at_end() {
            let (start, end) = (self.previous_start, self.previous_end);
            self.push_error_at("Expected end of file.", start, end);
        }
        self.clear_unused_annotations();
    }

    /// A string alone on a line at class level, which Godot accepts as a
    /// comment.
    fn parse_string_statement(&mut self) {
        self.start(STRING_STATEMENT);
        self.advance();
        if !self.eat(NEWLINE) {
            self.push_error("Expected newline after comment string.");
        }
        self.finish_node();
    }

    /// Godot's `parse_class`, after `class`.
    fn parse_class(&mut self) {
        let outer_extends_used = self.class_extends_used;
        self.class_extends_used = false;

        self.consume(
            IDENTIFIER,
            "Expected identifier for the class name after \"class\".",
        );
        if self.check(EXTENDS_KW) {
            self.start(EXTENDS);
            self.advance();
            self.parse_extends();
            self.finish_node();
        }
        self.consume(COLON, "Expected \":\" after class declaration.");

        self.start(CLASS_BODY);
        let multiline = self.eat(NEWLINE);
        if multiline && !self.consume(INDENT, "Expected indented block after class declaration.") {
            self.finish_node();
            self.class_extends_used = outer_extends_used;
            return;
        }
        if self.check(EXTENDS_KW) {
            self.start(EXTENDS);
            self.advance();
            if self.class_extends_used {
                self.push_error("Cannot use \"extends\" more than once in the same class.");
            }
            self.parse_extends();
            self.end_statement("superclass");
            self.finish_node();
        }
        self.parse_class_body(multiline);
        if multiline {
            self.consume(DEDENT, "Missing unindent at the end of the class body.");
        }
        self.finish_node();
        self.class_extends_used = outer_extends_used;
    }

    /// Godot's `parse_class_name`, after `class_name`.
    fn parse_class_name(&mut self) {
        self.consume(
            IDENTIFIER,
            "Expected identifier for the global class name after \"class_name\".",
        );
        if self.check(EXTENDS_KW) {
            // `extends` on the same line.
            self.start(EXTENDS);
            self.advance();
            self.parse_extends();
            self.finish_node();
            self.end_statement("superclass");
        } else {
            self.end_statement("class_name statement");
        }
    }

    /// Godot's `parse_extends`, after `extends`.
    fn parse_extends(&mut self) {
        self.class_extends_used = true;
        if self.check_literal() {
            self.advance();
            if self.previous != STRING {
                let type_name = variant_type_name(self.previous);
                self.push_error(format!(
                    "Only strings or identifiers can be used after \"extends\", found \"{type_name}\" instead."
                ));
            }
            if !self.eat(PERIOD) {
                return;
            }
        }
        if !self.consume(IDENTIFIER, "Expected superclass name after \"extends\".") {
            return;
        }
        while self.eat(PERIOD) {
            if !self.consume(IDENTIFIER, "Expected superclass name after \".\".") {
                return;
            }
        }
    }

    /// Start a member's node, taking in the annotations and `static` before
    /// it, then attach the pending annotations as Godot's
    /// `parse_class_member` does.
    fn start_member(&mut self, kind: SyntaxKind, target_kind: u32, member_kind: &str) {
        let checkpoint = self.member_checkpoint();
        self.start_at(checkpoint, kind);
        self.static_checkpoint = None;
        self.advance();
        self.attach_annotations(target_kind, &format!("a {member_kind}"));
    }

    /// Where a declaration starts: its first pending annotation, or `static`,
    /// or here.
    fn member_checkpoint(&mut self) -> Checkpoint {
        let parent = self.node_id();
        let annotation = self
            .annotation_stack
            .first()
            .filter(|a| a.parent == parent)
            .map(|a| a.checkpoint);
        let static_kw = self
            .static_checkpoint
            .filter(|&(_, p)| p == parent)
            .map(|(c, _)| c);
        match (annotation, static_kw) {
            (Some(a), _) => a,
            (None, Some(s)) => s,
            (None, None) => self.checkpoint(),
        }
    }

    /// Godot's annotation loop in `parse_class_member` and `parse_statement`.
    pub(super) fn attach_annotations(&mut self, target_kind: u32, what: &str) {
        while let Some(last) = self.annotation_stack.last() {
            if last.targets & target_kind != 0 {
                self.annotation_stack.pop();
            } else {
                let name = last.name.clone();
                self.push_error(format!(
                    "Annotation \"{name}\" cannot be applied to {what}."
                ));
                self.clear_unused_annotations();
            }
        }
    }

    /// Godot's `clear_unused_annotations`.
    pub(super) fn clear_unused_annotations(&mut self) {
        for annotation in std::mem::take(&mut self.annotation_stack) {
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

    /// Godot's `parse_class_body`.
    pub(super) fn parse_class_body(&mut self, multiline: bool) {
        let mut class_end = false;
        let mut next_is_static = false;
        while !class_end && !self.is_at_end() {
            let token = self.current;
            match token {
                VAR_KW => {
                    self.start_member(VAR, target::VARIABLE, "variable");
                    self.parse_variable(next_is_static, true);
                    self.finish_node();
                }
                CONST_KW => {
                    self.start_member(CONST, target::CONSTANT, "constant");
                    self.parse_constant();
                    self.finish_node();
                }
                SIGNAL_KW => {
                    self.start_member(SIGNAL, target::SIGNAL, "signal");
                    self.parse_signal();
                    self.finish_node();
                }
                FUNC_KW => {
                    self.start_member(FUNC, target::FUNCTION, "function");
                    self.parse_function(next_is_static);
                    self.finish_node();
                }
                CLASS_KW => {
                    self.start_member(CLASS, target::CLASS, "class");
                    self.parse_class();
                    self.finish_node();
                }
                ENUM_KW => {
                    self.start_member(ENUM, 0, "enum");
                    self.parse_enum();
                    self.finish_node();
                }
                STATIC_KW => {
                    let checkpoint = self.checkpoint();
                    self.static_checkpoint = Some((checkpoint, self.node_id()));
                    self.advance();
                    next_is_static = true;
                    if !self.check(FUNC_KW) && !self.check(VAR_KW) {
                        self.push_error("Expected \"func\" or \"var\" after \"static\".");
                    }
                }
                ANNOTATION => {
                    if let Some(annotation) =
                        self.parse_annotation(target::CLASS_LEVEL | target::STANDALONE)
                    {
                        if annotation.targets & target::STANDALONE != 0 {
                            if self.previous != NEWLINE {
                                self.push_error("Expected newline after a standalone annotation.");
                            }
                            if !is_export_group(&annotation.name)
                                && !is_warning_region(&annotation.name)
                            {
                                self.push_error("Unexpected standalone annotation.");
                            }
                        } else {
                            self.annotation_stack.push(annotation);
                        }
                    }
                }
                PASS_KW => {
                    self.start(PASS_STMT);
                    self.advance();
                    self.end_statement("\"pass\"");
                    self.finish_node();
                }
                DEDENT => class_end = true,
                STRING => self.parse_string_statement(),
                _ => {
                    self.start(ERROR);
                    self.advance();
                    let message = removed_keyword_message(self.previous_text())
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            format!(
                                "Unexpected {} in class body.",
                                debug_name(self.previous, self.previous_text())
                            )
                        });
                    self.push_error(message);
                    self.finish_node();
                }
            }
            if token != STATIC_KW {
                next_is_static = false;
            }
            if self.panic_mode {
                self.synchronize();
            }
            if !multiline {
                class_end = true;
            }
        }
    }

    /// Godot's `parse_variable`, after `var`.
    pub(super) fn parse_variable(&mut self, is_static: bool, allow_property: bool) {
        if !self.consume(IDENTIFIER, "Expected variable name after \"var\".") {
            return;
        }
        if self.eat(COLON) {
            if self.check(NEWLINE) {
                if allow_property {
                    self.advance();
                    self.parse_property(is_static, true);
                } else {
                    self.push_error("Expected type after \":\"");
                }
                return;
            } else if self.check(EQUAL) {
                // Inferred type.
            } else {
                if allow_property
                    && self.check(IDENTIFIER)
                    && matches!(self.current_text(), "get" | "set")
                {
                    self.parse_property(is_static, false);
                    return;
                }
                self.parse_type(false);
            }
        }
        if self.eat(EQUAL) && self.parse_expression(false).is_none() {
            self.push_error("Expected expression for variable initial value after \"=\".");
        }
        if allow_property && self.eat(COLON) {
            let need_indent = self.eat(NEWLINE);
            self.parse_property(is_static, need_indent);
            return;
        }
        self.end_statement("variable declaration");
    }

    /// Godot's `parse_property`: `get` and `set`, inline or as function names.
    fn parse_property(&mut self, is_static: bool, need_indent: bool) {
        self.start(PROPERTY);
        if need_indent && !self.consume(INDENT, "Expected indented block for property after \":\".")
        {
            self.finish_node();
            return;
        }
        let mut checkpoint = self.checkpoint();
        if !self.consume(
            IDENTIFIER,
            "Expected \"get\" or \"set\" for property declaration.",
        ) {
            self.finish_node();
            return;
        }
        let mut function = self.previous_text();
        let setget = self.check(EQUAL);
        if !setget && !need_indent {
            self.push_error("Property with inline code must go to an indented block.");
        }

        let (mut getter_used, mut setter_used) = (false, false);
        for i in 0..2 {
            match function {
                "set" if setter_used => self.push_error("Properties can only have one setter."),
                "set" => {
                    self.start_at(checkpoint, SETTER);
                    self.parse_property_setter(setget, is_static);
                    self.finish_node();
                    setter_used = true;
                }
                "get" if getter_used => self.push_error("Properties can only have one getter."),
                "get" => {
                    self.start_at(checkpoint, GETTER);
                    self.parse_property_getter(setget, is_static);
                    self.finish_node();
                    getter_used = true;
                }
                _ => self.push_error("Expected \"get\" or \"set\" for property declaration."),
            }
            if i == 0 && setget {
                if self.eat(COMMA) {
                    if self.eat(NEWLINE) && !need_indent {
                        self.push_error(
                            "Inline setter/getter setting cannot span across multiple lines (use \"\\\\\"\" if needed).",
                        );
                    }
                } else {
                    break;
                }
            }
            checkpoint = self.checkpoint();
            if !self.eat(IDENTIFIER) {
                break;
            }
            function = self.previous_text();
        }

        if setget {
            self.end_statement("property declaration");
        }
        if need_indent {
            self.consume(DEDENT, "Expected end of indented block for property.");
        }
        self.finish_node();
    }

    /// Godot's `parse_property_setter`, after `set`.
    fn parse_property_setter(&mut self, setget: bool, is_static: bool) {
        if setget {
            self.consume(EQUAL, "Expected \"=\" after \"set\"");
            self.consume(IDENTIFIER, "Expected setter function name after \"=\".");
            return;
        }
        self.start(PARAM_LIST);
        self.consume(PARENTHESIS_OPEN, "Expected \"(\" after \"set\".");
        let has_parameter = if self.check(IDENTIFIER) {
            self.start(PARAM);
            self.advance();
            self.finish_node();
            true
        } else {
            self.push_error("Expected parameter name after \"(\".");
            false
        };
        self.consume(PARENTHESIS_CLOSE, "Expected \")\" after parameter name.");
        self.finish_node();
        self.consume(COLON, "Expected \":\" after \")\".");
        if has_parameter {
            self.in_function(is_static, false, |p| {
                p.parse_suite("setter declaration", false)
            });
        }
    }

    /// Godot's `parse_property_getter`, after `get`.
    fn parse_property_getter(&mut self, setget: bool, is_static: bool) {
        if setget {
            self.consume(EQUAL, "Expected \"=\" after \"get\"");
            self.consume(IDENTIFIER, "Expected getter function name after \"=\".");
            return;
        }
        if self.eat(PARENTHESIS_OPEN) {
            self.consume(PARENTHESIS_CLOSE, "Expected \")\" after \"get(\".");
            self.consume(COLON, "Expected \":\" after \"get()\".");
        } else {
            self.consume(COLON, "Expected \":\" or \"(\" after \"get\".");
        }
        self.in_function(is_static, false, |p| {
            p.parse_suite("getter declaration", false)
        });
    }

    /// Godot's `parse_constant`, after `const`.
    pub(super) fn parse_constant(&mut self) {
        if !self.consume(IDENTIFIER, "Expected constant name after \"const\".") {
            return;
        }
        if self.eat(COLON) && !self.check(EQUAL) {
            self.parse_type(false);
        }
        if !self.consume(EQUAL, "Expected initializer after constant name.") {
            return;
        }
        if self.parse_expression(false).is_none() {
            self.push_error("Expected initializer expression for constant.");
            return;
        }
        self.end_statement("constant declaration");
    }

    /// Godot's `parse_parameter`. `start` is where the parameter node begins
    /// when a `...` was read before it.
    pub(super) fn parse_parameter(&mut self, start: Option<Checkpoint>) -> Option<Parameter> {
        if !self.check(IDENTIFIER) {
            self.push_error("Expected parameter name.");
            return None;
        }
        match start {
            Some(checkpoint) => self.start_at(checkpoint, PARAM),
            None => self.start(PARAM),
        }
        self.advance();
        if self.eat(COLON) && !self.check(EQUAL) {
            self.parse_type(false);
        }
        let has_default = self.eat(EQUAL);
        if has_default {
            self.parse_expression(false);
        }
        self.finish_node();
        Some(Parameter { has_default })
    }

    /// Godot's `parse_signal`, after `signal`.
    fn parse_signal(&mut self) {
        if !self.consume(IDENTIFIER, "Expected signal name after \"signal\".") {
            return;
        }
        if self.check(PARENTHESIS_OPEN) {
            self.start(PARAM_LIST);
            self.push_multiline(true);
            self.advance();
            loop {
                if self.check(PARENTHESIS_CLOSE) {
                    break; // Trailing comma.
                }
                let Some(parameter) = self.parse_parameter(None) else {
                    self.push_error("Expected signal parameter name.");
                    break;
                };
                if parameter.has_default {
                    self.push_error("Signal parameters cannot have a default value.");
                }
                if !self.eat(COMMA) || self.is_at_end() {
                    break;
                }
            }
            self.pop_multiline();
            self.consume(
                PARENTHESIS_CLOSE,
                "Expected closing \")\" after signal parameters.",
            );
            self.finish_node();
        }
        self.end_statement("signal declaration");
    }

    /// Godot's `parse_enum`, after `enum`.
    fn parse_enum(&mut self) {
        let named = self.eat(IDENTIFIER);
        self.push_multiline(true);
        let message = if named {
            "Expected \"{\" after enum name."
        } else {
            "Expected \"{\" after \"enum\"."
        };
        self.consume(BRACE_OPEN, message);
        loop {
            if self.check(BRACE_CLOSE) {
                break; // Trailing comma.
            }
            if self.check(IDENTIFIER) {
                self.start(ENUM_VALUE);
                self.advance();
                if self.eat(EQUAL) && self.parse_expression(false).is_none() {
                    self.push_error("Expected expression value after \"=\".");
                }
                self.finish_node();
            } else {
                self.push_error("Expected identifier for enum key.");
            }
            if !self.eat(COMMA) {
                break;
            }
        }
        self.pop_multiline();
        self.consume(BRACE_CLOSE, "Expected closing \"}\" for enum.");
        self.end_statement("enum");
    }

    /// Godot's `parse_function_signature`, after the opening parenthesis
    /// (and its `push_multiline`). Ends the parameter list node the caller
    /// started. Returns whether a `:` follows, so a body comes next.
    pub(super) fn parse_function_signature(&mut self, type_name: &str, name: Option<&str>) -> bool {
        let mut parameter_count = 0;
        let mut has_rest = false;
        if !self.check(PARENTHESIS_CLOSE) && !self.is_at_end() {
            let mut default_used = false;
            loop {
                if self.check(PARENTHESIS_CLOSE) {
                    break; // Trailing comma.
                }
                let checkpoint = self.checkpoint();
                let is_rest = self.eat(PERIOD_PERIOD_PERIOD);
                let Some(parameter) = self.parse_parameter(is_rest.then_some(checkpoint)) else {
                    break;
                };
                if has_rest {
                    self.push_error("Cannot have parameters after the rest parameter.");
                } else if parameter.has_default && is_rest {
                    self.push_error("The rest parameter cannot have a default value.");
                } else if !parameter.has_default && default_used && !is_rest {
                    self.push_error("Cannot have mandatory parameters after optional parameters.");
                } else {
                    default_used |= parameter.has_default;
                    if is_rest {
                        has_rest = true;
                    } else {
                        parameter_count += 1;
                    }
                }
                if !self.eat(COMMA) {
                    break;
                }
            }
        }
        self.pop_multiline();
        self.consume(
            PARENTHESIS_CLOSE,
            &format!("Expected closing \")\" after {type_name} parameters."),
        );
        self.finish_node(); // PARAM_LIST

        if self.check(FORWARD_ARROW) {
            self.start(RETURN_TYPE);
            self.advance();
            if self.parse_type(true).is_none() {
                self.push_error("Expected return type or \"void\" after \"->\".");
            }
            self.finish_node();
        }

        if name == Some("_static_init") {
            if !self.function.is_some_and(|f| f.is_static) {
                self.push_error("Static constructor must be declared static.");
            }
            if parameter_count > 0 || has_rest {
                self.push_error("Static constructor cannot have parameters.");
            }
        }

        if type_name == "lambda" {
            return self.consume(COLON, "Expected \":\" after lambda declaration.");
        }
        // Abstract functions have no colon and no body.
        self.eat(COLON)
    }

    /// Godot's `parse_function`, after `func`.
    fn parse_function(&mut self, is_static: bool) {
        if !self.consume(IDENTIFIER, "Expected function name after \"func\".") {
            return;
        }
        let name = self.previous_text();
        let is_constructor = matches!(name, "_init" | "_static_init");
        self.in_function(is_static, is_constructor, |p| {
            p.start(PARAM_LIST);
            p.push_multiline(true);
            p.consume(
                PARENTHESIS_OPEN,
                "Expected opening \"(\" after function name.",
            );
            let has_body = p.parse_function_signature("function", Some(name));
            if has_body {
                p.parse_suite("function declaration", false);
            } else {
                p.end_statement("bodyless function declaration");
            }
        });
    }

    /// Run `f` as Godot does with `current_function` set.
    pub(super) fn in_function(
        &mut self,
        is_static: bool,
        is_constructor: bool,
        f: impl FnOnce(&mut Self),
    ) {
        let previous = self.function.replace(super::FunctionContext {
            is_static,
            is_constructor,
        });
        f(self);
        self.function = previous;
    }

    /// Godot's `parse_annotation`, at the annotation token. Returns the
    /// annotation if it is valid where it is.
    pub(super) fn parse_annotation(&mut self, valid_targets: u32) -> Option<PendingAnnotation> {
        let checkpoint = self.checkpoint();
        let parent = self.node_id();
        self.start_at(checkpoint, ANNOTATION_NODE);
        let start = self.current_start;
        self.advance();
        let name = self.previous_text().to_string();

        let mut valid = true;
        let targets = annotation_targets(&name);
        match targets {
            None => {
                let message = match name.as_str() {
                    "@deprecated" => "\"@deprecated\" annotation does not exist. Use \"## @deprecated: Reason here.\" instead.".to_string(),
                    "@experimental" => "\"@experimental\" annotation does not exist. Use \"## @experimental: Reason here.\" instead.".to_string(),
                    "@tutorial" => "\"@tutorial\" annotation does not exist. Use \"## @tutorial(Title): https://example.com\" instead.".to_string(),
                    _ => format!("Unrecognized annotation: \"{name}\"."),
                };
                self.push_error(message);
                valid = false;
            }
            Some(targets) if targets & valid_targets == 0 => {
                if targets & target::SCRIPT != 0 {
                    self.push_error(format!(
                        "Annotation \"{name}\" must be at the top of the script, before \"extends\" and \"class_name\"."
                    ));
                } else {
                    self.push_error(format!(
                        "Annotation \"{name}\" is not allowed in this level."
                    ));
                }
                valid = false;
            }
            Some(_) => {}
        }

        if self.check(PARENTHESIS_OPEN) {
            self.start(ANNOTATION_ARGS);
            self.push_multiline(true);
            self.advance();
            loop {
                if self.check(PARENTHESIS_CLOSE) {
                    break; // Trailing comma.
                }
                if self.parse_expression(false).is_none() {
                    self.push_error("Expected expression as the annotation argument.");
                    valid = false;
                }
                if !self.eat(COMMA) {
                    break;
                }
            }
            self.pop_multiline();
            self.consume(
                PARENTHESIS_CLOSE,
                "Expected \")\" after annotation arguments.",
            );
            self.finish_node();
        }
        self.finish_node();
        let end = self.previous_end;
        self.eat(NEWLINE); // Optional.

        let targets = targets?;
        valid.then_some(PendingAnnotation {
            name,
            targets,
            checkpoint,
            parent,
            start,
            end,
        })
    }
}

fn is_export_group(name: &str) -> bool {
    matches!(
        name,
        "@export_category" | "@export_group" | "@export_subgroup"
    )
}

fn is_warning_region(name: &str) -> bool {
    matches!(name, "@warning_ignore_start" | "@warning_ignore_restore")
}

/// Godot's messages for Godot 3 keywords found in a class body.
fn removed_keyword_message(name: &str) -> Option<&'static str> {
    Some(match name {
        "export" => {
            "The \"export\" keyword was removed in Godot 4. Use an export annotation (\"@export\", \"@export_range\", etc.) instead."
        }
        "tool" => {
            "The \"tool\" keyword was removed in Godot 4. Use the \"@tool\" annotation instead."
        }
        "onready" => {
            "The \"onready\" keyword was removed in Godot 4. Use the \"@onready\" annotation instead."
        }
        "remote" => {
            "The \"remote\" keyword was removed in Godot 4. Use the \"@rpc\" annotation with \"any_peer\" instead."
        }
        "remotesync" => {
            "The \"remotesync\" keyword was removed in Godot 4. Use the \"@rpc\" annotation with \"any_peer\" and \"call_local\" instead."
        }
        "puppet" => {
            "The \"puppet\" keyword was removed in Godot 4. Use the \"@rpc\" annotation with \"authority\" instead."
        }
        "puppetsync" => {
            "The \"puppetsync\" keyword was removed in Godot 4. Use the \"@rpc\" annotation with \"authority\" and \"call_local\" instead."
        }
        "master" => {
            "The \"master\" keyword was removed in Godot 4. Use the \"@rpc\" annotation with \"any_peer\" and perform a check inside the function instead."
        }
        "mastersync" => {
            "The \"mastersync\" keyword was removed in Godot 4. Use the \"@rpc\" annotation with \"any_peer\" and \"call_local\", and perform a check inside the function instead."
        }
        _ => return None,
    })
}

/// Godot's `Token::get_debug_name`.
pub(super) fn debug_name(kind: SyntaxKind, text: &str) -> String {
    if kind == IDENTIFIER {
        format!("identifier \"{text}\"")
    } else {
        format!("\"{}\"", token_name(kind))
    }
}

/// Godot's `Variant::get_type_name` for a literal token.
fn variant_type_name(kind: SyntaxKind) -> &'static str {
    match kind {
        INT => "int",
        FLOAT => "float",
        TRUE_KW | FALSE_KW => "bool",
        NULL_KW => "Nil",
        STRING_NAME => "StringName",
        NODE_PATH => "NodePath",
        _ => "String",
    }
}
