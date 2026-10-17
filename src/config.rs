//! Validated, local preferences. GitHub credentials never belong in this file.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeName {
    #[default]
    Dark,
    Light,
    NoColor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub host: String,
    pub theme: ThemeName,
    pub refresh_seconds: u64,
    pub repository_sets: BTreeMap<String, Vec<String>>,
    pub active_set: Option<String>,
    pub excluded_repositories: Vec<String>,
    pub local_repositories: BTreeMap<String, PathBuf>,
    pub bindings: BTreeMap<String, String>,
    pub avatars: bool,
    pub max_file_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            host: "github.com".into(),
            theme: ThemeName::Dark,
            refresh_seconds: 60,
            repository_sets: BTreeMap::new(),
            active_set: None,
            excluded_repositories: vec![],
            local_repositories: BTreeMap::new(),
            bindings: BTreeMap::new(),
            avatars: true,
            max_file_bytes: 2 * 1024 * 1024,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(body) => {
                let config: Self = toml::from_str(&body)
                    .with_context(|| format!("Invalid configuration {}", path.display()))?;
                config.validate()?;
                Ok(config)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => {
                Err(error).with_context(|| format!("Read configuration {}", path.display()))
            }
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("Unsupported configuration version {}", self.version);
        }
        if self.host.is_empty()
            || self
                .host
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b".-:".contains(&b))
        {
            bail!("Host must be a hostname, without a URL scheme or whitespace");
        }
        if !(5..=86_400).contains(&self.refresh_seconds) {
            bail!("Refresh interval must be between 5 and 86400 seconds");
        }
        if self.max_file_bytes == 0 || self.max_file_bytes > 128 * 1024 * 1024 {
            bail!("File limit must be between 1 byte and 128 MiB");
        }
        if let Some(set) = &self.active_set {
            if !self.repository_sets.contains_key(set) {
                bail!("Selected repository set '{set}' does not exist");
            }
        }
        for (name, repositories) in &self.repository_sets {
            if name.trim().is_empty() {
                bail!("Repository sets need a name");
            }
            for repository in repositories {
                validate_repository(repository)?;
            }
        }
        for repository in &self.excluded_repositories {
            validate_repository(repository)?;
        }
        for repository in self.local_repositories.keys() {
            validate_repository(repository)?;
        }
        let mut bindings = default_bindings();
        for (action, key) in &self.bindings {
            if !bindings.contains_key(action) {
                bail!("Unknown remappable action '{action}'");
            }
            bindings.insert(action.clone(), canonical_key(key)?);
        }
        let mut used = BTreeMap::new();
        for (action, key) in &bindings {
            if let Some(other) = used.insert(key, action) {
                bail!("Key '{key}' is assigned to both {other} and {action}");
            }
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("Create configuration directory {}", parent.display()))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(toml::to_string_pretty(self)?.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn selected_repositories(&self) -> Option<Vec<&str>> {
        self.active_set
            .as_ref()
            .and_then(|name| self.repository_sets.get(name))
            .map(|repos| {
                repos
                    .iter()
                    .filter(|repo| !self.excluded_repositories.contains(repo))
                    .map(String::as_str)
                    .collect()
            })
    }

    pub fn resolved_bindings(&self) -> Result<BTreeMap<String, String>> {
        self.validate()?;
        let mut bindings = default_bindings();
        for (action, key) in &self.bindings {
            bindings.insert(action.clone(), canonical_key(key)?);
        }
        Ok(bindings)
    }
}

pub fn default_bindings() -> BTreeMap<String, String> {
    [
        ("inbox", "1"),
        ("repository", "2"),
        ("search", "3"),
        ("actions", "4"),
        ("dashboard", "5"),
        ("help", "?"),
        ("palette", ":"),
        ("back", "alt+left"),
        ("settings", "s"),
        ("refresh", "r"),
        ("quit", "q"),
    ]
    .into_iter()
    .map(|(action, key)| (action.into(), key.into()))
    .collect()
}

/// Portable terminal shortcuts: a character/named key, optionally ctrl+ or alt+.
pub fn canonical_key(value: &str) -> Result<String> {
    let mut key = value;
    let mut modifiers = Vec::new();
    while let Some((prefix, remaining)) = key.split_once('+') {
        if remaining.is_empty() {
            break;
        }
        match prefix.to_ascii_lowercase().as_str() {
            "ctrl" if !modifiers.contains(&"ctrl") => modifiers.push("ctrl"),
            "alt" if !modifiers.contains(&"alt") => modifiers.push("alt"),
            _ => bail!("Invalid shortcut '{value}'; use a character or ctrl+/alt+ key"),
        }
        key = remaining;
    }
    let named = [
        "enter", "esc", "tab", "backtab", "up", "down", "left", "right", "home", "end", "pageup",
        "pagedown", "space",
    ];
    let normalized = if key == " " {
        "space".to_owned()
    } else if key.chars().count() == 1 && !key.chars().any(char::is_control) {
        key.to_owned()
    } else if named.contains(&key.to_ascii_lowercase().as_str()) {
        key.to_ascii_lowercase()
    } else {
        bail!("Unsupported shortcut '{value}'");
    };
    if normalized == "esc"
        || normalized == "tab"
        || normalized == "backtab"
        || (modifiers.contains(&"ctrl") && normalized.eq_ignore_ascii_case("c"))
    {
        bail!("Shortcut '{value}' is reserved for cancellation, focus, or emergency quit");
    }
    modifiers.sort();
    if modifiers.is_empty() {
        Ok(normalized)
    } else {
        Ok(format!("{}+{normalized}", modifiers.join("+")))
    }
}

pub fn validate_repository(repository: &str) -> Result<()> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || part
                    .bytes()
                    .any(|b| !b.is_ascii_alphanumeric() && !b"._-".contains(&b))
        })
    {
        bail!("Repository '{repository}' must have owner/name format");
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub database: PathBuf,
    pub index: PathBuf,
    pub avatars: PathBuf,
}

impl Paths {
    pub fn discover(explicit: Option<PathBuf>) -> Result<Self> {
        let root = explicit
            .or_else(|| std::env::var_os("GHTUI_CONFIG_DIR").map(PathBuf::from))
            .or_else(default_config_directory)
            .context("Cannot find a configuration directory; use --config-dir")?;
        Ok(Self {
            config: root.join("config.toml"),
            database: root.join("cache.sqlite3"),
            index: root.join("index"),
            avatars: root.join("avatars"),
            root,
        })
    }
}

fn default_config_directory() -> Option<PathBuf> {
    match std::env::consts::OS {
        "macos" => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/ghtui")),
        "linux" => std::env::var_os("XDG_CONFIG_HOME")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|root| root.join("ghtui")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_roundtrip_keeps_scope_theme_and_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config
            .repository_sets
            .insert("Work".into(), vec!["org/a".into(), "org/b".into()]);
        config.active_set = Some("Work".into());
        config.excluded_repositories.push("org/b".into());
        config.theme = ThemeName::Light;
        config.bindings.insert("help".into(), "?".into());
        config.save(&path).unwrap();
        let read = Config::load(&path).unwrap();
        assert_eq!(config, read);
        assert_eq!(read.selected_repositories(), Some(vec!["org/a"]));
    }

    #[test]
    fn configuration_invalid_values_never_overwrite_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.save(&path).unwrap();
        let previous = fs::read(&path).unwrap();
        config.refresh_seconds = 0;
        assert!(config.save(&path).is_err());
        assert_eq!(previous, fs::read(path).unwrap());
    }

    #[test]
    fn configuration_rejects_key_collisions_and_bad_scope() {
        let mut config = Config::default();
        config.bindings.insert("help".into(), "x".into());
        config.bindings.insert("quit".into(), "x".into());
        assert!(config.validate().is_err());
        config.bindings.clear();
        config.active_set = Some("missing".into());
        assert!(config.validate().is_err());
        config.active_set = None;
        config.host = "https://github.com".into();
        assert!(config.validate().is_err());
        assert!(validate_repository("org/../repo").is_err());
    }

    #[test]
    fn configuration_custom_keys_validate_against_defaults() {
        let mut config = Config::default();
        config.bindings.insert("help".into(), "q".into());
        assert!(config.validate().is_err());
        config.bindings.insert("quit".into(), "ctrl+x".into());
        assert_eq!(config.resolved_bindings().unwrap()["help"], "q");
        assert_eq!(canonical_key("CTRL+ALT+x").unwrap(), "alt+ctrl+x");
        assert_eq!(canonical_key(" ").unwrap(), "space");
        assert!(canonical_key("ctrl+c").is_err());
        assert!(canonical_key("nonsense").is_err());
        config.bindings.insert("imaginary".into(), "z".into());
        assert!(config.validate().is_err());
    }

    #[test]
    fn configuration_missing_defaults_and_all_preferences_roundtrip_without_implicit_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences with spaces/config.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());
        assert!(!path.parent().unwrap().exists());
        let mapping = dir.path().join("local mapping 日本");
        let mut config = Config {
            host: "git.example.invalid".into(),
            theme: ThemeName::NoColor,
            refresh_seconds: 37,
            active_set: Some("Work".into()),
            excluded_repositories: vec!["org/b".into()],
            avatars: false,
            max_file_bytes: 7 * 1024 * 1024,
            ..Config::default()
        };
        config.repository_sets.insert(
            "Work".into(),
            vec!["org/a".into(), "org/b".into(), "org/c".into()],
        );
        config
            .repository_sets
            .insert("Personal".into(), vec!["user/notes".into()]);
        config
            .local_repositories
            .insert("org/a".into(), mapping.clone());
        config.bindings.insert("help".into(), "h".into());
        config.bindings.insert("quit".into(), "ctrl+x".into());
        config.save(&path).unwrap();
        let saved = fs::read(&path).unwrap();
        let restored = Config::load(&path).unwrap();
        assert_eq!(restored, config);
        assert_eq!(
            restored.selected_repositories(),
            Some(vec!["org/a", "org/c"])
        );
        assert_eq!(restored.resolved_bindings().unwrap()["quit"], "ctrl+x");
        assert_eq!(
            fs::read(&path).unwrap(),
            saved,
            "Loading must not rewrite preferences"
        );
        assert!(
            !mapping.exists(),
            "Saving a mapping is not permission to create or clone it"
        );
    }

    #[test]
    fn configuration_invalid_limits_and_unknown_fields_preserve_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config::default();
        config.save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        let invalid: [fn(&mut Config); 6] = [
            |c| c.refresh_seconds = 4,
            |c| c.refresh_seconds = 86_401,
            |c| c.max_file_bytes = 0,
            |c| c.max_file_bytes = 128 * 1024 * 1024 + 1,
            |c| c.version = 2,
            |c| c.host = "https://github.com".into(),
        ];
        for change in invalid {
            let mut candidate = config.clone();
            change(&mut candidate);
            assert!(candidate.save(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(Config::load(&path).unwrap(), config);
        }
        let invalid_path = dir.path().join("invalid.toml");
        for bytes in [
            b"version = \"not a number\"\n".as_slice(),
            b"imaginary_preference = true\n",
        ] {
            fs::write(&invalid_path, bytes).unwrap();
            let error = Config::load(&invalid_path).unwrap_err().to_string();
            assert!(error.contains("Invalid configuration"));
            assert!(error.contains("invalid.toml"));
            assert_eq!(fs::read(&invalid_path).unwrap(), bytes);
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }
}
