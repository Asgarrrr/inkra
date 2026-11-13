//! `.inkra/settings.json`, parsed once into every setting the app reads.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;

use inkra_vault::{CONFIG_DIR, Fs};
use serde_json::Value;

use crate::theme::store::{Choice, describe, read_text};

#[derive(Default)]
pub(crate) struct Settings {
    pub(crate) choice: Choice,
    pub(crate) code_block_wrap: bool,
    pub(crate) features: Switches,
}

/// `features` in `settings.json`: a feature's switch, by id; an absent id keeps its default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Switches(BTreeMap<String, bool>);

impl Switches {
    pub(crate) fn get(&self, id: &str) -> Option<bool> {
        self.0.get(id).copied()
    }
}

/// Never fails: a missing or broken file reads as the defaults and says why.
pub(crate) async fn read(fs: &dyn Fs, config: &Path, notices: &mut Vec<String>) -> Settings {
    let text = match read_text(fs, &config.join("settings.json")).await {
        Ok(text) => text,
        Err(error) => {
            if error.io_kind() != Some(ErrorKind::NotFound) {
                notices.push(error.to_string());
            }
            return Settings::default();
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => parse(&value),
        Err(error) => {
            notices.push(describe(&format!("{CONFIG_DIR}/settings.json"), &error));
            Settings::default()
        }
    }
}

fn parse(value: &Value) -> Settings {
    let features = value
        .get("features")
        .and_then(Value::as_object)
        .map(|members| {
            members
                .iter()
                .filter_map(|(id, on)| Some((id.clone(), on.as_bool()?)))
                .collect()
        })
        .unwrap_or_default();
    Settings {
        choice: Choice::from_setting(value.get("theme").and_then(Value::as_str)),
        code_block_wrap: value
            .get("code_block_wrap")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        features: Switches(features),
    }
}

#[cfg(test)]
mod tests;
