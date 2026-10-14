//! Argument parsing remains independent of terminal initialization.
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Command {
    #[default]
    Run,
    Help,
    Version,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub command: Command,
    pub config_dir: Option<PathBuf>,
    pub host: Option<String>,
    pub repository: Option<String>,
    pub no_color: bool,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--version" | "-V" => options.command = Command::Version,
                "--help" | "-h" => options.command = Command::Help,
                "--no-color" => options.no_color = true,
                "--config-dir" => options.config_dir = Some(PathBuf::from(value(&mut args, &arg)?)),
                "--host" => options.host = Some(value(&mut args, &arg)?),
                "--repo" => options.repository = Some(value(&mut args, &arg)?),
                _ => return Err(format!("Unknown argument {arg:?}; use --help")),
            }
        }
        Ok(options)
    }
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .filter(|value| !value.is_empty() && !value.starts_with("--"))
        .ok_or_else(|| format!("{flag} needs a value"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|arg| (*arg).into()))
    }

    #[test]
    fn bootstrap_arguments_handle_help_version_and_scoped_options() {
        assert_eq!(parse(&["--help"]).unwrap().command, Command::Help);
        assert_eq!(parse(&["--version"]).unwrap().command, Command::Version);
        let options = parse(&[
            "--config-dir",
            "/tmp/ghtui fixture",
            "--host",
            "git.example.com",
            "--repo",
            "org/repo",
            "--no-color",
        ])
        .unwrap();
        assert_eq!(
            options.config_dir,
            Some(PathBuf::from("/tmp/ghtui fixture"))
        );
        assert_eq!(options.host.as_deref(), Some("git.example.com"));
        assert_eq!(options.repository.as_deref(), Some("org/repo"));
        assert!(options.no_color);
    }

    #[test]
    fn bootstrap_arguments_reject_unknown_flags_and_missing_values() {
        assert!(parse(&["--repo"]).unwrap_err().contains("needs a value"));
        assert!(parse(&["--config-dir", "--help"]).is_err());
        assert!(parse(&["--made-up"])
            .unwrap_err()
            .contains("Unknown argument"));
    }
}
