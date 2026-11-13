use gpui::Rgba;

/// Lists every token once: the struct, `set` and the test list cannot drift apart.
macro_rules! tokens {
    ($($(#[$doc:meta])* $name:ident => $variant:ident,)*) => {
        /// Every colour a theme sets, named by role, not by the place that reads it.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub(crate) struct ThemeColors {
            $($(#[$doc])* pub(crate) $name: Rgba,)*
        }

        /// A token by value, so a decoration names its colour and the theme resolves it.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[expect(dead_code, reason = "a variant exists for every token; only those a decoration names are built")]
        pub(crate) enum ColorToken {
            $($variant,)*
        }

        impl ThemeColors {
            pub(crate) fn color(&self, token: ColorToken) -> Rgba {
                match token {
                    $(ColorToken::$variant => self.$name,)*
                }
            }

            #[cfg(test)]
            pub(crate) const NAMES: &[&str] = &[$(stringify!($name)),*];

            /// False for a name no token has.
            pub(crate) fn set(&mut self, name: &str, color: Rgba) -> bool {
                match name {
                    $(stringify!($name) => self.$name = color,)*
                    _ => return false,
                }
                true
            }

            #[cfg(test)]
            pub(crate) fn get(&self, name: &str) -> Option<Rgba> {
                match name {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }
        }
    };
}

tokens! {
    /// The chrome: sidebar, tab bar, backlinks.
    background => Background,
    editor_background => EditorBackground,
    /// A surface over the others: a modal.
    elevated_background => ElevatedBackground,
    border => Border,
    border_variant => BorderVariant,
    element_hover => ElementHover,
    element_selected => ElementSelected,
    chrome_element_hover => ChromeElementHover,
    ghost_element_hover => GhostElementHover,
    accent => Accent,
    accent_background => AccentBackground,
    selection => Selection,
    error_background => ErrorBackground,
    warning_background => WarningBackground,
    md_text => MdText,
    md_heading => MdHeading,
    md_code => MdCode,
    /// A code block's text; inline code keeps `md_code`.
    md_code_block => MdCodeBlock,
    /// Behind a code block.
    md_code_block_background => MdCodeBlockBackground,
    md_link => MdLink,
    md_quote => MdQuote,
    md_quote_bar => MdQuoteBar,
    md_marker => MdMarker,
    text => Text,
    text_selected => TextSelected,
    text_muted => TextMuted,
    text_placeholder => TextPlaceholder,
    /// A search match.
    text_highlight => TextHighlight,
    error => Error,
    warning => Warning,
}
