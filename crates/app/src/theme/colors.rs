use gpui::Rgba;

/// Every colour a theme sets, named by role, not by the place that reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThemeColors {
    /// The chrome: sidebar, tab bar, backlinks.
    pub(crate) background: Rgba,
    pub(crate) editor_background: Rgba,
    /// A surface over the others: a modal.
    pub(crate) elevated_background: Rgba,
    pub(crate) border: Rgba,
    pub(crate) border_variant: Rgba,
    pub(crate) element_hover: Rgba,
    pub(crate) element_selected: Rgba,
    pub(crate) chrome_element_hover: Rgba,
    pub(crate) ghost_element_hover: Rgba,
    pub(crate) accent: Rgba,
    pub(crate) accent_background: Rgba,
    pub(crate) selection: Rgba,
    pub(crate) error_background: Rgba,
    pub(crate) warning_background: Rgba,
    pub(crate) md_text: Rgba,
    pub(crate) md_heading: Rgba,
    pub(crate) md_code: Rgba,
    pub(crate) md_link: Rgba,
    pub(crate) md_quote: Rgba,
    pub(crate) md_quote_bar: Rgba,
    pub(crate) md_marker: Rgba,
    pub(crate) text: Rgba,
    pub(crate) text_selected: Rgba,
    pub(crate) text_muted: Rgba,
    pub(crate) text_placeholder: Rgba,
    /// A search match.
    pub(crate) text_highlight: Rgba,
    pub(crate) error: Rgba,
    pub(crate) warning: Rgba,
}
