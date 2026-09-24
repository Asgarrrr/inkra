use inkra_markdown::{Link, LinkKind};
use percent_encoding::percent_decode_str;

use crate::key::strip_md;

/// What a link points at, before resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Note {
        /// As written, `.md` stripped, `/`-separated.
        path: String,
        fragment: Option<String>,
        style: Style,
    },
    /// A URL; opened, never resolved.
    External(String),
    /// `[[#Heading]]`: the source itself, never a backlink.
    SelfLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    /// A wikilink with no `/`: any note of that name.
    Name,
    /// A wikilink with a `/`: a note whose path ends with it.
    Suffix,
    /// An inline link, or a wikilink starting `./`, `../` or `/`: from the source's folder, or
    /// from the vault root after a leading `/`.
    Relative,
}

impl Target {
    #[must_use]
    pub fn parse(link: &Link) -> Self {
        let (path, fragment) = match link.target.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment.to_owned())),
            None => (link.target.as_str(), None),
        };
        let decoded;
        let path = match link.kind {
            LinkKind::Inline if has_scheme(&link.target) => {
                return Self::External(link.target.clone());
            }
            LinkKind::Inline => {
                decoded = percent_decode_str(path).decode_utf8_lossy();
                decoded.as_ref()
            }
            LinkKind::Wiki => path,
        };
        let path = strip_md(path.trim());
        if path.is_empty() {
            return Self::SelfLink;
        }
        let style = match link.kind {
            LinkKind::Inline => Style::Relative,
            LinkKind::Wiki if path.starts_with(['.', '/']) && is_relative(path) => Style::Relative,
            LinkKind::Wiki if path.contains('/') => Style::Suffix,
            LinkKind::Wiki => Style::Name,
        };
        Self::Note {
            path: path.to_owned(),
            fragment,
            style,
        }
    }
}

fn is_relative(path: &str) -> bool {
    path.starts_with("./") || path.starts_with("../") || path.starts_with('/')
}

/// RFC 3986: a letter, then letters, digits, `+`, `-` or `.`, then `:`. Two characters at
/// least, so a drive letter such as `C:` is not one.
fn has_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && scheme.len() >= 2
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests;
