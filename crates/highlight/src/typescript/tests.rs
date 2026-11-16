use crate::Language::{JavaScript, Tsx, TypeScript};
use crate::TokenClass::{
    self, Constant, Function, Keyword, Operator, Property, Punctuation, Tag, Type,
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
