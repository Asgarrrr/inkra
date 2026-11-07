use gpui::Rgba;

/// Lists every token once: the struct, `set` and the test list cannot drift apart.
macro_rules! tokens {
    ($($(#[$doc:meta])* $name:ident,)*) => {
        /// Every colour a theme sets, named by role, not by the place that reads it.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub(crate) struct ThemeColors {
            $($(#[$doc])* pub(crate) $name: Rgba,)*
        }

        impl ThemeColors {
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
    background,
    editor_background,
    /// A surface over the others: a modal.
    elevated_background,
    border,
    border_variant,
    element_hover,
    element_selected,
    chrome_element_hover,
    ghost_element_hover,
    accent,
    accent_background,
    selection,
    error_background,
    warning_background,
    md_text,
    md_heading,
    md_code,
    /// A code block's text; inline code keeps `md_code`.
    md_code_block,
    /// Behind a code block.
    md_code_block_background,
    md_link,
    md_quote,
    md_quote_bar,
    md_marker,
    text,
    text_selected,
    text_muted,
    text_placeholder,
    /// A search match.
    text_highlight,
    error,
    warning,
}
