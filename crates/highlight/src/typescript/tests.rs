use crate::Language::{JavaScript, Tsx, TypeScript};
use crate::TokenClass::{
    self, Attribute, Constant, Function, Keyword, Operator, Property, Punctuation, String, Tag,
    Type,
};
use crate::fixtures::{expect, tokens};
use crate::scan::GENERIC_LIMIT;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(TypeScript, text, expected);
}

#[test]
fn a_generic_arrow_is_a_generic_in_typescript_and_a_tag_where_jsx_is_on() {
    let text = "<T>(x: T) => x";
    check(
        text,
        &[
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("(", Punctuation),
            (":", Punctuation),
            ("T", Type),
            (")", Punctuation),
            ("=>", Operator),
        ],
    );
    for language in [Tsx, JavaScript] {
        expect(
            language,
            text,
            &[("<", Punctuation), ("T", Tag), (">", Punctuation)],
        );
    }
}

#[test]
fn a_type_keyword_names_a_type_and_a_generic_list_holds_types() {
    check(
        "interface Point<T> extends Base<T> {",
        &[
            ("interface", Keyword),
            ("Point", Type),
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("extends", Keyword),
            ("Base", Type),
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("{", Punctuation),
        ],
    );
    check(
        "export abstract class A implements B {}",
        &[
            ("export", Keyword),
            ("abstract", Keyword),
            ("class", Keyword),
            ("A", Type),
            ("implements", Keyword),
            ("B", Type),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_type_keyword_is_a_name_where_no_operand_follows_and_in_javascript() {
    check(
        "type Id = string; x = { type: 1 }.type; x.readonly = private",
        &[
            ("type", Keyword),
            ("Id", Type),
            ("=", Operator),
            ("string", Type),
            (";", Punctuation),
            ("=", Operator),
            ("{", Punctuation),
            ("type", Property),
            (":", Punctuation),
            ("1", Constant),
            ("}", Punctuation),
            (".", Punctuation),
            ("type", Property),
            (";", Punctuation),
            (".", Punctuation),
            ("readonly", Property),
            ("=", Operator),
        ],
    );
    expect(JavaScript, "type Id = string", &[("=", Operator)]);
}

#[test]
fn an_annotation_after_a_parameter_or_a_declared_name_is_a_type() {
    check(
        "function f(a: foo, b?: Bar): number {}",
        &[
            ("function", Keyword),
            ("f", Function),
            ("(", Punctuation),
            (":", Punctuation),
            ("foo", Type),
            (",", Punctuation),
            ("?", Punctuation),
            (":", Punctuation),
            ("Bar", Type),
            (")", Punctuation),
            (":", Punctuation),
            ("number", Type),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
    check(
        "let n: count = 1",
        &[
            ("let", Keyword),
            (":", Punctuation),
            ("count", Type),
            ("=", Operator),
            ("1", Constant),
        ],
    );
    expect(
        JavaScript,
        "let n: count",
        &[("let", Keyword), (":", Punctuation)],
    );
}

#[test]
fn a_ternary_branch_in_a_call_is_no_annotation() {
    check(
        "f(a ? b : c)",
        &[
            ("f", Function),
            ("(", Punctuation),
            ("?", Operator),
            (":", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn a_call_with_type_arguments_is_a_function_in_typescript_only() {
    check(
        "useState<string>(x)",
        &[
            ("useState", Function),
            ("<", Punctuation),
            ("string", Type),
            (">", Punctuation),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
    expect(
        JavaScript,
        "useState<string>(x)",
        &[
            ("<", Operator),
            (">", Operator),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn nested_generic_lists_close_one_angle_at_a_time() {
    check(
        "let m: Map<string, Array<number>> = new Map()",
        &[
            ("let", Keyword),
            (":", Punctuation),
            ("Map", Type),
            ("<", Punctuation),
            ("string", Type),
            (",", Punctuation),
            ("Array", Type),
            ("<", Punctuation),
            ("number", Type),
            (">", Punctuation),
            (">", Punctuation),
            ("=", Operator),
            ("new", Keyword),
            ("Map", Type),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn a_less_than_after_a_lower_case_value_compares() {
    check(
        "if (a < b) {}",
        &[
            ("if", Keyword),
            ("(", Punctuation),
            ("<", Operator),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_capitalised_name_is_a_type_in_typescript_and_plain_in_javascript() {
    let text = "Foo.bar as Baz";
    check(
        text,
        &[
            ("Foo", Type),
            (".", Punctuation),
            ("bar", Property),
            ("as", Keyword),
            ("Baz", Type),
        ],
    );
    expect(
        JavaScript,
        text,
        &[(".", Punctuation), ("bar", Property), ("as", Keyword)],
    );
}

#[test]
fn tsx_keeps_generic_lists_after_a_type_and_opens_tags_at_an_expression_start() {
    expect(
        Tsx,
        "const el: Array<T> = <div />",
        &[
            ("const", Keyword),
            (":", Punctuation),
            ("Array", Type),
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("=", Operator),
            ("<", Punctuation),
            ("div", Tag),
            ("/>", Punctuation),
        ],
    );
}

#[test]
fn a_member_after_its_modifiers_is_a_property_and_an_optional_one_annotates_its_type() {
    check(
        "{ private readonly id: number; title?: foo; body: T }",
        &[
            ("{", Punctuation),
            ("private", Keyword),
            ("readonly", Keyword),
            ("id", Property),
            (":", Punctuation),
            ("number", Type),
            (";", Punctuation),
            ("title", Property),
            ("?", Punctuation),
            (":", Punctuation),
            ("foo", Type),
            (";", Punctuation),
            ("body", Property),
            (":", Punctuation),
            ("T", Type),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_generic_call_closes_within_the_look_ahead_limit() {
    // `f<AAA…>(x)`, its list `len` bytes from `<` to `>`.
    let call = |len: usize| format!("f<{}>(x)", "A".repeat(len - 2));
    assert_eq!(
        tokens(TypeScript, &call(GENERIC_LIMIT))[0],
        (0, 1, Function)
    );
    assert_eq!(
        tokens(TypeScript, &call(GENERIC_LIMIT + 1))[0],
        (1, 2, Operator)
    );
}

#[test]
fn a_generic_list_does_not_close_past_a_semicolon_or_a_brace() {
    for stop in [";", "{", "}"] {
        let text = format!("a < b {stop} c > (d)");
        assert_eq!(tokens(TypeScript, &text)[0], (2, 3, Operator), "{text:?}");
    }
}

/// The tokens of `let a = 2;`, a line after the one a test is about.
const NEXT_LINE: [(&str, TokenClass); 4] = [
    ("let", Keyword),
    ("=", Operator),
    ("2", Constant),
    (";", Punctuation),
];

#[track_caller]
fn check_tsx_then_next_line(line: &str, expected: &[(&str, TokenClass)]) {
    let wanted: Vec<_> = expected.iter().chain(&NEXT_LINE).copied().collect();
    expect(Tsx, &format!("{line}\nlet a = 2;"), &wanted);
}

#[test]
fn tsx_reads_a_trailing_comma_or_extends_as_type_parameters_not_a_tag() {
    let arrow = [
        ("(", Punctuation),
        (":", Punctuation),
        ("T", Type),
        (")", Punctuation),
        ("=>", Operator),
        (";", Punctuation),
    ];
    let comma = [
        ("const", Keyword),
        ("=", Operator),
        ("<", Punctuation),
        ("T", Type),
        (",", Punctuation),
        (">", Punctuation),
    ];
    check_tsx_then_next_line(
        "const id = <T,>(x: T) => x;",
        &[&comma[..], &arrow].concat(),
    );
    let extends = [
        ("const", Keyword),
        ("=", Operator),
        ("<", Punctuation),
        ("T", Type),
        ("extends", Keyword),
        ("object", Type),
        (">", Punctuation),
    ];
    check_tsx_then_next_line(
        "const f = <T extends object>(x: T) => x;",
        &[&extends[..], &arrow].concat(),
    );
    let asynchronous = [
        ("const", Keyword),
        ("=", Operator),
        ("async", Keyword),
        ("<", Punctuation),
        ("T", Type),
        (",", Punctuation),
        (">", Punctuation),
    ];
    check_tsx_then_next_line(
        "const f = async <T,>(x: T) => x;",
        &[&asynchronous[..], &arrow].concat(),
    );
}

#[test]
fn a_type_alias_value_is_a_type_where_a_less_than_opens_type_parameters() {
    check_tsx_then_next_line(
        "type F = <T>(x: T) => T;",
        &[
            ("type", Keyword),
            ("F", Type),
            ("=", Operator),
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("(", Punctuation),
            (":", Punctuation),
            ("T", Type),
            (")", Punctuation),
            ("=>", Operator),
            ("T", Type),
            (";", Punctuation),
        ],
    );
    // `type` with no `=` on its line leaves the next line's `=` an assignment.
    expect(
        Tsx,
        "import type Foo from \"./foo\"\nconst el = <b />;",
        &[
            ("import", Keyword),
            ("type", Keyword),
            ("Foo", Type),
            ("from", Keyword),
            ("\"./foo\"", String),
            ("const", Keyword),
            ("=", Operator),
            ("<", Punctuation),
            ("b", Tag),
            ("/>", Punctuation),
            (";", Punctuation),
        ],
    );
    // `type {` names no alias, so the `=` after it on the line is an assignment.
    check_tsx_then_next_line(
        "export type { F }; x = <b />;",
        &[
            ("export", Keyword),
            ("type", Keyword),
            ("{", Punctuation),
            ("F", Type),
            ("}", Punctuation),
            (";", Punctuation),
            ("=", Operator),
            ("<", Punctuation),
            ("b", Tag),
            ("/>", Punctuation),
            (";", Punctuation),
        ],
    );
    // An initialiser after an annotation is an expression: `<` opens a tag there.
    check_tsx_then_next_line(
        "const el: F = <b />;",
        &[
            ("const", Keyword),
            (":", Punctuation),
            ("F", Type),
            ("=", Operator),
            ("<", Punctuation),
            ("b", Tag),
            ("/>", Punctuation),
            (";", Punctuation),
        ],
    );
}

#[test]
fn a_less_than_after_a_type_compares_when_no_list_closes_on_the_line() {
    check(
        "const ok = Count < limit;",
        &[
            ("const", Keyword),
            ("=", Operator),
            ("Count", Type),
            ("<", Operator),
            (";", Punctuation),
        ],
    );
}

#[test]
fn javascript_reads_a_jsx_attribute_named_extends_as_an_attribute() {
    expect(
        JavaScript,
        "<Tip extends />",
        &[
            ("<", Punctuation),
            ("Tip", Tag),
            ("extends", Attribute),
            ("/>", Punctuation),
        ],
    );
}

#[test]
fn a_generic_list_past_the_nesting_room_is_operators_and_the_next_line_still_colours() {
    let room = crate::state::LineState::default().stack.len();
    let text = format!("{}f<T>(x)\nconst a = 1;", "(".repeat(room));
    let expected: Vec<_> = std::iter::repeat_n(("(", Punctuation), room)
        .chain([
            ("f", Function),
            ("<", Operator),
            ("T", Type),
            (">", Operator),
            ("(", Punctuation),
            (")", Punctuation),
            ("const", Keyword),
            ("=", Operator),
            ("1", Constant),
            (";", Punctuation),
        ])
        .collect();
    check(&text, &expected);
}

#[test]
fn a_member_after_one_modifier_or_a_decorator_is_a_property() {
    check(
        "class A { @Input() private name: string }",
        &[
            ("class", Keyword),
            ("A", Type),
            ("{", Punctuation),
            ("@Input", Attribute),
            ("(", Punctuation),
            (")", Punctuation),
            ("private", Keyword),
            ("name", Property),
            (":", Punctuation),
            ("string", Type),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_generic_list_holds_keywords_and_literals_and_a_paren_closes_it() {
    check(
        "let p: Pick<T, keyof T> | Promise<undefined>;\n(X<B) > (c)",
        &[
            ("let", Keyword),
            (":", Punctuation),
            ("Pick", Type),
            ("<", Punctuation),
            ("T", Type),
            (",", Punctuation),
            ("keyof", Keyword),
            ("T", Type),
            (">", Punctuation),
            ("|", Operator),
            ("Promise", Type),
            ("<", Punctuation),
            ("undefined", Constant),
            (">", Punctuation),
            (";", Punctuation),
            ("(", Punctuation),
            ("X", Type),
            ("<", Punctuation),
            ("B", Type),
            (")", Punctuation),
            (">", Operator),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn an_object_literal_value_is_no_annotation() {
    check(
        "x = { key: value };",
        &[
            ("=", Operator),
            ("{", Punctuation),
            ("key", Property),
            (":", Punctuation),
            ("}", Punctuation),
            (";", Punctuation),
        ],
    );
}
