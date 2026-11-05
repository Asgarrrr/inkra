//! The vault's theme choice in `.inkra/settings.json`, resolved to a theme (D2).

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use gpui::SharedString;
use inkra_vault::{CONFIG_DIR, Fs, VaultError};
use serde_json::Value;

use super::{Theme, builtin, file};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum Choice {
    #[default]
    System,
    /// A built-in's name, else the stem of a file in `.inkra/themes/`.
    Named(SharedString),
}

#[derive(Default)]
pub(crate) struct Resolved {
    /// `None` follows the system.
    pub(crate) theme: Option<Arc<Theme>>,
    pub(crate) notices: Vec<String>,
}

/// Never fails: a missing or broken file falls back to the system theme and says why.
pub(crate) async fn load(fs: &dyn Fs, root: &Path) -> Resolved {
    let config = root.join(CONFIG_DIR);
    let mut notices = Vec::new();
    let theme = match choice(fs, &config, &mut notices).await {
        Choice::System => None,
        Choice::Named(name) => named(fs, &config, &name, &mut notices).await,
    };
    Resolved { theme, notices }
}

async fn choice(fs: &dyn Fs, config: &Path, notices: &mut Vec<String>) -> Choice {
    let text = match fs.load(&config.join("settings.json")).await {
        Ok(loaded) => loaded.text,
        Err(error) => {
            if !is_not_found(&error) {
                notices.push(error.to_string());
            }
            return Choice::System;
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(settings) => match settings.get("theme").and_then(Value::as_str) {
            Some(name) if name != "system" => Choice::Named(name.to_owned().into()),
            _ => Choice::System,
        },
        Err(error) => {
            notices.push(file::describe(
                &format!("{CONFIG_DIR}/settings.json"),
                &error,
            ));
            Choice::System
        }
    }
}

async fn named(
    fs: &dyn Fs,
    config: &Path,
    name: &str,
    notices: &mut Vec<String>,
) -> Option<Arc<Theme>> {
    if let Some(theme) = builtin::named(name) {
        return Some(theme.clone());
    }
    let not_found = || format!("theme \"{name}\" not found");
    // A name is a stem in `themes/`, never a path out of it.
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        notices.push(not_found());
        return None;
    }
    let path = config.join("themes").join(format!("{name}.json"));
    let text = match fs.load(&path).await {
        Ok(loaded) => loaded.text,
        Err(error) if is_not_found(&error) => {
            notices.push(not_found());
            return None;
        }
        Err(error) => {
            notices.push(error.to_string());
            return None;
        }
    };
    let label = format!("theme \"{name}.json\"");
    match file::parse(&text) {
        Ok(parsed) => {
            if !parsed.skipped.is_empty() {
                notices.push(format!("{label}: skipped {}", parsed.skipped.join(", ")));
            }
            Some(Arc::new(parsed.theme))
        }
        Err(error) => {
            notices.push(file::describe(&label, &error));
            None
        }
    }
}

fn is_not_found(error: &VaultError) -> bool {
    matches!(error, VaultError::Io { source, .. } if source.kind() == ErrorKind::NotFound)
}

#[cfg(test)]
mod tests;
