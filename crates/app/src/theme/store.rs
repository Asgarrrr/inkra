//! The vault's theme choice in `.inkra/settings.json`, resolved to a theme (D2).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{AppContext as _, Context, SharedString, Task};
use inkra_vault::{CONFIG_DIR, Fs, RelPath, VaultError};
use serde_json::Value;

use super::{Theme, builtin, file};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum Choice {
    #[default]
    System,
    /// A built-in's name, else the stem of a file in `.inkra/themes/`.
    Named(SharedString),
}

impl Choice {
    /// A theme file by this stem is never listed: its name reads as this choice.
    const SYSTEM: &str = "system";

    /// `theme` in `settings.json`; a missing one follows the system.
    fn from_setting(name: Option<&str>) -> Self {
        match name {
            Some(name) if name != Self::SYSTEM => Self::Named(name.to_owned().into()),
            _ => Self::System,
        }
    }

    fn setting(&self) -> &str {
        match self {
            Self::System => Self::SYSTEM,
            Self::Named(name) => name,
        }
    }
}

#[derive(Default)]
pub(crate) struct Resolved {
    /// As read, even when its theme was not found.
    pub(crate) choice: Choice,
    /// `None` follows the system.
    pub(crate) theme: Option<Arc<Theme>>,
    pub(crate) notices: Vec<String>,
}

/// The adopted choice, and the tasks that load, list and write it.
#[derive(Default)]
pub(crate) struct ThemeStore {
    choice: Choice,
    /// Held: a newer reload or a pick drops it, so a stale one never lands.
    reload: Option<Task<()>>,
    /// The vault's themes, listed before the picker opens.
    listing: Option<Task<()>>,
    /// The last write of the choice; the next one awaits it, so two picks land in order.
    write: Option<Task<()>>,
}

impl ThemeStore {
    pub(crate) fn choice(&self) -> &Choice {
        &self.choice
    }

    pub(crate) fn adopt(&mut self, choice: Choice) {
        self.choice = choice;
    }

    pub(crate) fn reload(&mut self, task: Task<()>) {
        self.reload = Some(task);
    }

    pub(crate) fn listing(&mut self, task: Task<()>) {
        self.listing = Some(task);
    }

    /// Adopts `choice` over any reload in flight, and writes it to `vault` after the previous write.
    pub(crate) fn pick<V: 'static>(
        &mut self,
        choice: Choice,
        vault: Option<(Arc<dyn Fs>, PathBuf)>,
        cx: &mut Context<V>,
        failed: fn(&mut V, String, &mut Context<V>),
    ) {
        self.choice = choice.clone();
        self.reload = None;
        let Some((fs, root)) = vault else {
            return;
        };
        let previous = self.write.take();
        self.write = Some(cx.spawn(async move |view, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let written = cx
                .background_spawn(async move { write_choice(&*fs, &root, &choice).await })
                .await;
            if let Err(text) = written {
                view.update(cx, |view, cx| failed(view, text, cx)).ok();
            }
        }));
    }

    /// Whether a change at `path` can change what `load` resolves.
    pub(crate) fn concerns(&self, path: &RelPath) -> bool {
        let config = Path::new(CONFIG_DIR);
        let path = path.as_path();
        if path == config || path == config.join("settings.json") {
            return true;
        }
        match &self.choice {
            Choice::Named(name) if builtin::named(name).is_none() => {
                let themes = config.join("themes");
                path == themes || path == themes.join(format!("{name}.json"))
            }
            Choice::System | Choice::Named(_) => false,
        }
    }
}

/// Never fails: a missing or broken file falls back to the system theme and says why.
pub(crate) async fn load(fs: &dyn Fs, root: &Path) -> Resolved {
    let config = root.join(CONFIG_DIR);
    let mut notices = Vec::new();
    let choice = choice(fs, &config, &mut notices).await;
    let theme = match &choice {
        Choice::System => None,
        Choice::Named(name) => named(fs, &config, name, &mut notices).await,
    };
    Resolved {
        choice,
        theme,
        notices,
    }
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
        Ok(settings) => Choice::from_setting(settings.get("theme").and_then(Value::as_str)),
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

/// The vault's parsable themes by stem, sorted; a built-in's name, `system` and a broken file are left out.
pub(crate) async fn list(fs: &dyn Fs, root: &Path) -> Vec<(SharedString, Arc<Theme>)> {
    let Ok(files) = fs.list_files(&root.join(CONFIG_DIR).join("themes")).await else {
        return Vec::new();
    };
    let mut themes = Vec::new();
    for path in files {
        let json = path
            .extension()
            .is_some_and(|extension| extension == "json");
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !json || stem == Choice::SYSTEM || builtin::named(stem).is_some() {
            continue;
        }
        let Ok(loaded) = fs.load(&path).await else {
            continue;
        };
        if let Ok(parsed) = file::parse(&loaded.text) {
            themes.push((SharedString::from(stem.to_owned()), Arc::new(parsed.theme)));
        }
    }
    themes.sort_by(|(a, _), (b, _)| a.cmp(b));
    themes
}

/// Sets `theme` in `settings.json` and keeps every other key; a file that is not an object is left alone.
pub(crate) async fn write_choice(fs: &dyn Fs, root: &Path, choice: &Choice) -> Result<(), String> {
    let failed =
        |reason: &dyn std::fmt::Display| format!("Couldn't save the theme choice: {reason}");
    let path = root.join(CONFIG_DIR).join("settings.json");
    let not_an_object = format!("{CONFIG_DIR}/settings.json is not a JSON object");
    let mut settings = match fs.load(&path).await {
        Ok(loaded) => serde_json::from_str::<Value>(&loaded.text)
            .ok()
            .filter(Value::is_object)
            .ok_or_else(|| failed(&not_an_object))?,
        Err(error) if is_not_found(&error) => Value::Object(serde_json::Map::new()),
        Err(error) => return Err(failed(&error)),
    };
    settings["theme"] = Value::from(choice.setting());
    let mut text = serde_json::to_string_pretty(&settings).map_err(|error| failed(&error))?;
    text.push('\n');
    fs.replace_bytes(&path, text.as_bytes())
        .await
        .map_err(|error| failed(&error))
}

fn is_not_found(error: &VaultError) -> bool {
    matches!(error, VaultError::Io { source, .. } if source.kind() == ErrorKind::NotFound)
}

#[cfg(test)]
mod tests;
