//! Default GitHub repository for the checkout ghtui was launched in.
//! Only a remote URL on the configured host is used; it is a scope default
//! for repository views, not proof of access (each read still goes via gh).
use std::path::Path;

/// `owner/name` from `origin` (or the only remote) when it targets `host`.
/// One explicit read-only config query, no `git remote` subcommand.
pub fn detect(directory: &Path, host: &str) -> Option<String> {
    let text = String::from_utf8(crate::git::remote_url_config_v1(directory)?).ok()?;
    let remotes: Vec<(&str, &str)> = text
        .split('\0')
        .filter_map(|entry| {
            let (key, url) = entry.split_once('\n')?;
            let name = key.strip_prefix("remote.")?.strip_suffix(".url")?;
            Some((name, url))
        })
        .collect();
    let url = match remotes.iter().find(|(name, _)| *name == "origin") {
        Some((_, url)) => *url,
        None if remotes.len() == 1 => remotes[0].1,
        None => return None,
    };
    parse(url, host)
}

/// Accepts `git@host:owner/name(.git)`, `ssh://[user@]host[:port]/owner/name`,
/// and `https://[user@]host/owner/name`. Host comparison ignores ASCII case.
pub fn parse(url: &str, host: &str) -> Option<String> {
    let url = url.trim();
    let (url_host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !matches!(scheme, "https" | "http" | "ssh" | "git" | "git+ssh") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let authority = authority.split_once(':').map_or(authority, |(h, _)| h);
        (authority, path)
    } else {
        // scp-like syntax: [user@]host:path
        let (authority, path) = url.split_once(':')?;
        if authority.contains('/') {
            return None;
        }
        (
            authority.rsplit_once('@').map_or(authority, |(_, h)| h),
            path,
        )
    };
    if !url_host.eq_ignore_ascii_case(host) {
        return None;
    }
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let repository = path.to_owned();
    crate::config::validate_repository(&repository)
        .ok()
        .map(|()| repository)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_default_parses_common_github_remote_forms() {
        for url in [
            "git@github.com:octo/Repo.git",
            "git@github.com:octo/Repo",
            "ssh://git@github.com/octo/Repo.git",
            "ssh://git@github.com:22/octo/Repo",
            "https://github.com/octo/Repo.git",
            "https://user@GitHub.com/octo/Repo/",
        ] {
            assert_eq!(
                parse(url, "github.com").as_deref(),
                Some("octo/Repo"),
                "{url}"
            );
        }
    }

    #[test]
    fn origin_default_rejects_other_hosts_and_non_repository_paths() {
        for url in [
            "git@gitlab.com:octo/repo.git",
            "https://github.example.com/octo/repo",
            "https://github.com/octo",
            "https://github.com/octo/repo/tree/main",
            "/srv/git/octo/repo.git",
            "file:///srv/octo/repo.git",
            "../octo/repo",
        ] {
            assert_eq!(parse(url, "github.com"), None, "{url}");
        }
        assert_eq!(
            parse("git@ghe.corp:team/app.git", "ghe.corp").as_deref(),
            Some("team/app")
        );
    }

    #[test]
    fn origin_default_detects_origin_or_single_remote_in_a_real_checkout() {
        let root = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(root.path())
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        };
        run(&["init", "-q"]);
        assert_eq!(detect(root.path(), "github.com"), None);
        run(&["remote", "add", "upstream", "git@github.com:up/stream.git"]);
        assert_eq!(
            detect(root.path(), "github.com").as_deref(),
            Some("up/stream")
        );
        run(&["remote", "add", "fork", "git@github.com:me/stream.git"]);
        assert_eq!(
            detect(root.path(), "github.com"),
            None,
            "ambiguous without origin"
        );
        run(&[
            "remote",
            "add",
            "origin",
            "https://github.com/me/stream.git",
        ]);
        assert_eq!(
            detect(root.path(), "github.com").as_deref(),
            Some("me/stream")
        );
        assert_eq!(detect(root.path(), "ghe.corp"), None);
    }
}
