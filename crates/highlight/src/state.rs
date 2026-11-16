//! What a lexer carries from one line to the next (code highlighting spec, D2).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) open: Open,
    pub(crate) prev: Prev,
    /// The open nestings, outermost first; every entry from `depth` on is `Nest::default()`, so
    /// equal nestings compare equal.
    pub(crate) stack: [Nest; 16],
    /// Counts past 16 too, where nothing is stored and the top is unknown.
    pub(crate) depth: u8,
    /// The depth of the open attribute's entry.
    pub(crate) attribute: Option<u8>,
    /// Bash's pending heredocs in body order; the first one's body is read first.
    pub(crate) heredocs: [Option<Heredoc>; 2],
}

impl LineState {
    /// Whether a push stores its entry.
    pub(crate) fn room(&self) -> bool {
        usize::from(self.depth) < self.stack.len()
    }

    pub(crate) fn push(&mut self, nest: Nest) {
        if let Some(slot) = self.stack.get_mut(usize::from(self.depth)) {
            *slot = nest;
        }
        self.depth = self.depth.saturating_add(1);
    }

    pub(crate) fn pop(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        if let Some(slot) = self.stack.get_mut(usize::from(self.depth)) {
            *slot = Nest::default();
        }
    }

    /// The innermost entry, known only at a depth of 1 to 16.
    pub(crate) fn top(&self) -> Option<Nest> {
        let at = usize::from(self.depth).checked_sub(1)?;
        self.stack.get(at).copied()
    }
}

/// What is open at a line end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Open {
    #[default]
    Nothing,
    BlockComment {
        depth: u8,
    },
    /// `None` is an escaped string.
    Str {
        raw_hashes: Option<u8>,
    },
    /// A string that closes on `quote`, thrice when `triple`; a `\\` skips a byte when `escapes`.
    Quoted {
        quote: u8,
        triple: bool,
        escapes: bool,
    },
}

/// A Bash heredoc whose body has not ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Heredoc {
    /// The tag's hash, quotes removed; `tag_hash` of the line that ends the body.
    pub(crate) tag: u64,
    /// `<<-`: leading tabs are stripped from each body line and from the closing tag.
    pub(crate) strip_tabs: bool,
    /// An unquoted tag: the body expands `$` parameters.
    pub(crate) expands: bool,
}

/// What a nesting stack entry opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Nest {
    #[default]
    Paren,
    Bracket,
    Brace,
    Generic,
    /// A TypeScript generic list opened after a type position: its `>` leaves one (S4 deviation
    /// 19).
    TypeGeneric,
    /// A TypeScript generic list opened after a declared function's name: its `>` leaves the name
    /// (S4 deviation 20).
    FnGeneric,
    /// TypeScript's `(` after `function`, a declared function's name or a method's: its `)` leaves
    /// `Returns`.
    Params,
    /// A TypeScript return type, from its `:` to the body's `{`, a `;`, a `,` or a closer; it
    /// crosses a line only under a nesting opened after it.
    ReturnType,
    Attribute,
    /// Bash's `((` or `$((`.
    Arith,
    /// Bash's opening `` ` ``.
    Backtick,
    /// Bash's `(` right after an assignment's `=`; `declare` when the look-behind was `Declare`.
    Array {
        declare: bool,
    },
    /// JavaScript's `(` right after `if`, `while`, `for` or `with`: its `)` ends no value.
    Condition,
    /// TypeScript's `{` in a type position: an interface, class, type literal, `namespace` or
    /// `enum` body, whose members' `:` start a type (S4 deviation 19).
    TypeBody,
    /// A TypeScript type alias from `type` to its `=`; it crosses a line only under a nesting
    /// opened after it.
    Alias,
    /// A JavaScript template string's text; a `${` above it pushes a `Brace`.
    Template,
    /// A JSX opening tag, from its `<` to its `>` or `/>`.
    JsxTag,
    /// A JSX closing tag, from its `</` to its `>`.
    JsxClosingTag,
    /// A JSX element's children, from its opening tag's `>` to its closing tag's `</`.
    JsxText,
    /// HTML's `<script>` content, lexed by JavaScript; always the bottom entry.
    Script,
    /// HTML's `<style>` content, lexed by CSS; always the bottom entry.
    Style,
}

/// The previous significant token, D4's one-token look-behind; whitespace and comments leave it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Prev {
    #[default]
    Other,
    /// A lone `.`, not `..` or `...`.
    Dot,
    Fn,
    /// `struct`, `enum`, `trait`, `type` or `impl`.
    TypeKeyword,
    /// An identifier just classed `Type`.
    Type,
    /// An identifier just classed `Type` in a type position, or a `TypeGeneric`'s `>`: in
    /// TypeScript, a `<` opens a generic list that may close on a later line (S4 deviation 18).
    TypePosition,
    /// The identifier right after `fn`; in JavaScript, after `function`, or a method's name.
    FnName,
    /// `::`.
    PathSep,
    /// Bash, past a command's name: the next word is an argument. `Other` is a command position.
    Command,
    /// Bash, past `export`, `local`, `declare` or `unset`: names and assignments. JavaScript, past
    /// `let`, `const` or `var`.
    Declare,
    /// Bash, past `for` or `select`: the loop variable.
    For,
    /// Bash, past the loop variable: `in`, or an argument.
    ForIn,
    /// Bash, past `case`: the subject.
    Case,
    /// Bash, past the subject: `in`, or an argument.
    CaseIn,
    /// Bash, a case item's pattern, up to its `)`.
    Pattern,
    /// Bash, inside `[` or `[[`.
    Test,
    /// JavaScript, a value's end: a `/` divides and a `<` compares (D4's expression start).
    ValueEnd,
    /// JavaScript, past `if`, `while`, `for` or `with`: the next `(` opens a `Condition`.
    Condition,
    /// JavaScript, past `{`, `(`, `,` or `;`: an identifier before `:` is a key or a parameter.
    KeyStart,
    /// TypeScript, a parameter, a declared name or a member before its `:` or `?:`.
    Param,
    /// TypeScript, past a `Params`'s `)`: a `:` starts a return type.
    Returns,
    /// TypeScript, past a `Param`'s `:` or an alias's `=`: the next identifier is a type.
    Annotation,
    /// CSS, inside a selector.
    Selector,
    /// CSS, past a property's name, up to its `;` or `}`.
    Declaration,
    /// CSS, past an at-rule's keyword, up to its `{` or `;`.
    AtRule,
    /// HTML, inside a tag.
    HtmlTag,
    /// HTML, inside a `<script>` opening tag: its `>` opens JavaScript.
    HtmlScript,
    /// HTML, inside a `<style>` opening tag: its `>` opens CSS.
    HtmlStyle,
}
