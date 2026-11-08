//! The pulldown-cmark settings every event walker shares, so all of them read a note alike.

use pulldown_cmark::Options;

pub(crate) fn options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
}
