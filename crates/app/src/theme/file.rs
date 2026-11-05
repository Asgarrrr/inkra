//! A user theme file: its `appearance`'s built-in, with any subset of tokens overridden (D1, D9).

use gpui::Rgba;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{Appearance, Theme, builtin};

/// Any other top-level key is ignored, so a file written for a later version still loads.
#[derive(Deserialize)]
struct File {
    appearance: FileAppearance,
    #[serde(default)]
    colors: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum FileAppearance {
    Light,
    Dark,
}

pub(crate) struct Parsed {
    pub(crate) theme: Theme,
    /// Sorted: unknown names and values that are not a colour.
    pub(crate) skipped: Vec<String>,
}

/// Only a JSON error or a missing `appearance` rejects the file; a bad key keeps the base value.
pub(crate) fn parse(text: &str) -> Result<Parsed, serde_json::Error> {
    let file: File = serde_json::from_str(text)?;
    let appearance = match file.appearance {
        FileAppearance::Light => Appearance::Light,
        FileAppearance::Dark => Appearance::Dark,
    };
    let mut colors = builtin::of(appearance).colors;
    let mut skipped: Vec<String> = file
        .colors
        .into_iter()
        .filter(|(name, value)| {
            !value
                .as_str()
                .and_then(parse_color)
                .is_some_and(|color| colors.set(name, color))
        })
        .map(|(name, _)| name)
        .collect();
    skipped.sort();
    Ok(Parsed {
        theme: Theme { appearance, colors },
        skipped,
    })
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` exactly: `Rgba::try_from` also trims and takes `#rgba` and a sign.
pub(crate) fn parse_color(text: &str) -> Option<Rgba> {
    let hex = text.strip_prefix('#')?;
    if !matches!(hex.len(), 3 | 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Rgba::try_from(text).ok()
}

/// `{label}: line L: message`, without serde's own ` at line L column C`.
pub(crate) fn describe(label: &str, error: &serde_json::Error) -> String {
    let message = error.to_string();
    let message = message
        .rsplit_once(" at line ")
        .map_or(message.as_str(), |(message, _)| message);
    format!("{label}: line {}: {message}", error.line())
}

#[cfg(test)]
mod tests;
