use crate::Language::{JavaScript, Tsx, TypeScript};
use crate::TokenClass::{
    self, Attribute, Constant, Function, Keyword, Operator, Property, Punctuation, String, Tag,
    Type,
};
use crate::fixtures::{expect, tokens};

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
fn a_generic_call_closes_at_any_distance_on_its_line() {
    // `f<AAA…>(x)`, its list `len` bytes from `<` to `>`.
    let call = |len: usize| format!("f<{}>(x)", "A".repeat(len - 2));
    for len in [2, 65, 4096] {
        assert_eq!(tokens(TypeScript, &call(len))[0], (0, 1, Function), "{len}");
    }
}

#[test]
fn a_generic_list_does_not_close_past_a_semicolon_or_a_brace() {
    // Near the `<` and past the look-ahead, where the line's pairing answers.
    for pad in [0, 100] {
        for stop in [";", "{", "}"] {
            let text = format!("a < b{} {stop} c > (d)", " ".repeat(pad));
            assert_eq!(tokens(TypeScript, &text)[0], (2, 3, Operator), "{text:?}");
        }
    }
}

#[test]
fn a_long_generic_list_closes_at_its_own_angle() {
    // The inner list closes first, the outer one past the look-ahead.
    let text = format!("f<A<B>{}>(x)", ", C".repeat(40));
    let tokens = tokens(TypeScript, &text);
    assert_eq!(tokens[0], (0, 1, Function));
    let close = text.len() - 4;
    assert!(
        tokens.contains(&(close, close + 1, Punctuation)),
        "{tokens:?}"
    );
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

#[test]
fn a_generic_list_in_a_type_position_spans_lines() {
    check(
        "type Handler = Map<\n  string,\n  Array<Request>\n>;\nlet a = 2;",
        &[
            ("type", Keyword),
            ("Handler", Type),
            ("=", Operator),
            ("Map", Type),
            ("<", Punctuation),
            ("string", Type),
            (",", Punctuation),
            ("Array", Type),
            ("<", Punctuation),
            ("Request", Type),
            (">", Punctuation),
            (">", Punctuation),
            (";", Punctuation),
        ]
        .into_iter()
        .chain(NEXT_LINE)
        .collect::<Vec<_>>(),
    );
    check(
        "function f(x: Record<\n  K,\n  V\n>) {}",
        &[
            ("function", Keyword),
            ("f", Function),
            ("(", Punctuation),
            (":", Punctuation),
            ("Record", Type),
            ("<", Punctuation),
            ("K", Type),
            (",", Punctuation),
            ("V", Type),
            (">", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn type_parameters_across_lines_keep_the_alias_and_the_class_body() {
    check(
        "type Pair<\n  A,\n  B,\n> = [A, B];",
        &[
            ("type", Keyword),
            ("Pair", Type),
            ("<", Punctuation),
            ("A", Type),
            (",", Punctuation),
            ("B", Type),
            (",", Punctuation),
            (">", Punctuation),
            ("=", Operator),
            ("[", Punctuation),
            ("A", Type),
            (",", Punctuation),
            ("B", Type),
            ("]", Punctuation),
            (";", Punctuation),
        ],
    );
    check(
        "class Store<\n  T,\n> extends Base<T> {\n  items: T;\n}",
        &[
            ("class", Keyword),
            ("Store", Type),
            ("<", Punctuation),
            ("T", Type),
            (",", Punctuation),
            (">", Punctuation),
            ("extends", Keyword),
            ("Base", Type),
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("{", Punctuation),
            ("items", Property),
            (":", Punctuation),
            ("T", Type),
            (";", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn nested_lists_and_a_trailing_comma_span_lines() {
    check(
        "type X = A<B<\n  C\n>>;\nlet a = 2;",
        &[
            ("type", Keyword),
            ("X", Type),
            ("=", Operator),
            ("A", Type),
            ("<", Punctuation),
            ("B", Type),
            ("<", Punctuation),
            ("C", Type),
            (">", Punctuation),
            (">", Punctuation),
            (";", Punctuation),
        ]
        .into_iter()
        .chain(NEXT_LINE)
        .collect::<Vec<_>>(),
    );
    check(
        "let r: Record<string,\n  V>;",
        &[
            ("let", Keyword),
            (":", Punctuation),
            ("Record", Type),
            ("<", Punctuation),
            ("string", Type),
            (",", Punctuation),
            ("V", Type),
            (">", Punctuation),
            (";", Punctuation),
        ],
    );
}

#[test]
fn a_semicolon_or_a_brace_ends_a_list_left_open_across_lines() {
    for (stop, line) in [("}", "}"), (";", "  K;")] {
        let mut expected = vec![
            ("let", Keyword),
            (":", Punctuation),
            ("Map", Type),
            ("<", Punctuation),
        ];
        if stop == ";" {
            expected.push(("K", Type));
        }
        expected.push((stop, Punctuation));
        expected.extend(NEXT_LINE);
        check(&format!("let m: Map<\n{line}\nlet a = 2;"), &expected);
    }
}

#[test]
fn a_comparison_after_new_without_semicolons_stays_on_its_line() {
    check(
        "const late = new Date < deadline\nlet a = 2;",
        &[
            ("const", Keyword),
            ("=", Operator),
            ("new", Keyword),
            ("Date", Type),
            ("<", Operator),
        ]
        .into_iter()
        .chain(NEXT_LINE)
        .collect::<Vec<_>>(),
    );
}

#[test]
fn outside_a_type_position_a_list_must_close_on_its_line() {
    // An expression, a union member, an `as` type and a return type.
    for text in [
        "f<\n  T>(x)",
        "let u: A | B<\n  T>(x)",
        "y = x as B<\n  T>(x)",
        "function g(): B<\n  T>(x)",
    ] {
        let tokens = tokens(TypeScript, text);
        let angle = text.find('<').unwrap();
        assert!(
            tokens.contains(&(angle, angle + 1, Operator)),
            "{text:?}: {tokens:?}"
        );
        let close = text.find('>').unwrap();
        assert!(
            tokens.contains(&(close, close + 1, Operator)),
            "{text:?}: {tokens:?}"
        );
    }
}

#[test]
fn a_comparison_after_an_as_types_list_is_an_operator() {
    let text = "ok = v as Box<T> < limit,\n  w";
    let compare = text.find(" < ").unwrap() + 1;
    let tokens = tokens(TypeScript, text);
    assert!(
        tokens.contains(&(compare, compare + 1, Operator)),
        "{tokens:?}"
    );
}

#[test]
fn an_interface_member_annotation_spans_lines() {
    check(
        "interface Store<T> {\n  items: Map<\n    K,\n    V\n  >;\n  name: label;\n}\nlet a = 2;",
        &[
            ("interface", Keyword),
            ("Store", Type),
            ("<", Punctuation),
            ("T", Type),
            (">", Punctuation),
            ("{", Punctuation),
            ("items", Property),
            (":", Punctuation),
            ("Map", Type),
            ("<", Punctuation),
            ("K", Type),
            (",", Punctuation),
            ("V", Type),
            (">", Punctuation),
            (";", Punctuation),
            ("name", Property),
            (":", Punctuation),
            ("label", Type),
            (";", Punctuation),
            ("}", Punctuation),
        ]
        .into_iter()
        .chain(NEXT_LINE)
        .collect::<Vec<_>>(),
    );
}

#[test]
fn a_type_literal_and_a_class_body_annotate_their_members() {
    check(
        "type P = { id: ident };\nlet q: { at: spot } = x;",
        &[
            ("type", Keyword),
            ("P", Type),
            ("=", Operator),
            ("{", Punctuation),
            ("id", Property),
            (":", Punctuation),
            ("ident", Type),
            ("}", Punctuation),
            (";", Punctuation),
            ("let", Keyword),
            (":", Punctuation),
            ("{", Punctuation),
            ("at", Property),
            (":", Punctuation),
            ("spot", Type),
            ("}", Punctuation),
            ("=", Operator),
            (";", Punctuation),
        ],
    );
    check(
        "class A extends B {\n  m: Map<\n    K\n  >;\n  render() { return { key: value }; }\n}",
        &[
            ("class", Keyword),
            ("A", Type),
            ("extends", Keyword),
            ("B", Type),
            ("{", Punctuation),
            ("m", Property),
            (":", Punctuation),
            ("Map", Type),
            ("<", Punctuation),
            ("K", Type),
            (">", Punctuation),
            (";", Punctuation),
            ("render", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("return", Keyword),
            ("{", Punctuation),
            ("key", Property),
            (":", Punctuation),
            ("}", Punctuation),
            (";", Punctuation),
            ("}", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn an_object_literal_and_a_function_body_are_no_type_body() {
    for text in [
        "const o = { key: value };",
        "function f(): Foo { return { key: value }; }",
        "function g(): string { label: value; }",
        // The body's `}` closed it, so the label after it is no member.
        "type T = { a: A };\nlabel: value;",
        // Lists opened after no type position: an assertion and a return type.
        "const o = <Options>{ key: value };",
        "function h(): Promise<T> { label: value; }",
    ] {
        let tokens = tokens(TypeScript, text);
        let value = text.find("value").unwrap();
        assert!(
            tokens.iter().all(|&(start, _, _)| start != value),
            "{text:?}: {tokens:?}"
        );
    }
}
