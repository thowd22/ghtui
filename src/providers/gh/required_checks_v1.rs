//! Required status checks for a PR's base branch (TASK-14 AC #1).
//!
//! Required contexts come from two read-only REST sources that any account
//! with read access can see: classic branch protection on the base branch
//! (`GET repos/{repo}/branches/{base}`) and the active rulesets that apply to
//! it (`GET repos/{repo}/rules/branches/{base}`). Both must be read for the
//! answer to be known; when either cannot be read the result is explicitly
//! `Unavailable` and no check is called optional.
use super::*;

/// Where GitHub declared a context as required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequiredSourceV1 {
    BranchProtection,
    Ruleset,
}

/// One required context. `app_id` is the app the rule pins, when it pins one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredContextV1 {
    pub context: String,
    pub app_id: Option<u64>,
    pub source: RequiredSourceV1,
}

/// The required-check answer for one base branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequiredChecksV1 {
    /// Both sources were read; `contexts` is the complete required set (may be empty).
    Known {
        base_ref: String,
        contexts: Vec<RequiredContextV1>,
    },
    /// At least one source could not be read; nothing is claimed optional.
    Unavailable { base_ref: String, reason: String },
}

/// How a required context stands on the observed head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequiredStateV1 {
    Passed,
    Failing,
    Pending,
    /// No check run or status with this name was reported for the head.
    NotReported,
}

const MAX_CONTEXTS: usize = 500;

impl RequiredChecksV1 {
    pub fn base_ref(&self) -> &str {
        match self {
            Self::Known { base_ref, .. } | Self::Unavailable { base_ref, .. } => base_ref,
        }
    }
    /// `Some(true)` required, `Some(false)` optional, `None` unknown.
    pub fn required(&self, check: &Check) -> Option<bool> {
        match self {
            Self::Known { contexts, .. } => Some(contexts.iter().any(|c| c.context == check.name)),
            Self::Unavailable { .. } => None,
        }
    }
    /// Required contexts and their state on the observed checks, in rule order.
    pub fn states<'a>(&'a self, checks: &[Check]) -> Vec<(&'a str, RequiredStateV1)> {
        let Self::Known { contexts, .. } = self else {
            return Vec::new();
        };
        contexts
            .iter()
            .map(|required| {
                let matching = checks.iter().filter(|c| c.name == required.context);
                let mut state = RequiredStateV1::NotReported;
                for check in matching {
                    let this = check_state(check);
                    state = match (state, this) {
                        (RequiredStateV1::Failing, _) | (_, RequiredStateV1::Failing) => {
                            RequiredStateV1::Failing
                        }
                        (RequiredStateV1::Pending, _) | (_, RequiredStateV1::Pending) => {
                            RequiredStateV1::Pending
                        }
                        _ => RequiredStateV1::Passed,
                    };
                }
                (required.context.as_str(), state)
            })
            .collect()
    }
    /// One line for the Checks tab header, with the role that colours it.
    pub fn summary(&self, checks: &[Check]) -> (String, crate::theme::SemanticToken) {
        use crate::theme::SemanticToken;
        match self {
            Self::Unavailable { base_ref, reason } => (
                format!("Required checks on {base_ref} unavailable: {reason}"),
                SemanticToken::Muted,
            ),
            Self::Known { base_ref, contexts } if contexts.is_empty() => (
                format!("No required checks on {base_ref}"),
                SemanticToken::Muted,
            ),
            Self::Known { base_ref, .. } => {
                let states = self.states(checks);
                let count = |s| states.iter().filter(|(_, x)| *x == s).count();
                let (failing, pending, passed, missing) = (
                    count(RequiredStateV1::Failing),
                    count(RequiredStateV1::Pending),
                    count(RequiredStateV1::Passed),
                    count(RequiredStateV1::NotReported),
                );
                let parts: Vec<String> = [
                    (failing, "failing"),
                    (pending, "pending"),
                    (missing, "not reported"),
                    (passed, "passed"),
                ]
                .into_iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, word)| format!("{n} {word}"))
                .collect();
                let role = if failing > 0 {
                    SemanticToken::Failure
                } else if pending + missing > 0 {
                    SemanticToken::Pending
                } else {
                    SemanticToken::Success
                };
                (
                    format!("Required on {base_ref}: {}", parts.join(", ")),
                    role,
                )
            }
        }
    }
}

/// The state one observed check contributes to a required context.
pub fn check_state(check: &Check) -> RequiredStateV1 {
    match check.conclusion.as_str() {
        "success" | "neutral" | "skipped" => RequiredStateV1::Passed,
        "" => RequiredStateV1::Pending,
        _ => RequiredStateV1::Failing,
    }
}

fn push(
    out: &mut Vec<RequiredContextV1>,
    context: &str,
    app_id: Option<u64>,
    source: RequiredSourceV1,
) -> Result<(), &'static str> {
    if context.is_empty() || context.len() > 1024 || context.chars().any(char::is_control) {
        return Err("a required context had no usable name");
    }
    if out.iter().any(|c| c.context == context) {
        return Ok(());
    }
    if out.len() >= MAX_CONTEXTS {
        return Err("too many required contexts");
    }
    out.push(RequiredContextV1 {
        context: context.into(),
        app_id,
        source,
    });
    Ok(())
}

/// Classic branch protection from `GET repos/{repo}/branches/{branch}`.
pub fn parse_branch_protection(
    value: &Value,
    out: &mut Vec<RequiredContextV1>,
) -> Result<(), &'static str> {
    let Some(protected) = value.get("protected").and_then(Value::as_bool) else {
        return Err("branch protection state was not returned");
    };
    if !protected {
        return Ok(());
    }
    let protection = value.get("protection");
    if protection
        .and_then(|p| p.get("enabled"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Ok(());
    }
    let Some(required) = protection.and_then(|p| p.get("required_status_checks")) else {
        return Err("branch protection details were not returned");
    };
    match required.get("enforcement_level").and_then(Value::as_str) {
        Some("off") => return Ok(()),
        Some(_) => {}
        None => return Err("branch protection enforcement was not returned"),
    }
    if let Some(checks) = required.get("checks") {
        let Some(checks) = checks.as_array() else {
            return Err("branch protection checks were malformed");
        };
        for check in checks {
            let Some(context) = check.get("context").and_then(Value::as_str) else {
                return Err("branch protection check had no context");
            };
            let app_id = check.get("app_id").and_then(Value::as_u64);
            push(out, context, app_id, RequiredSourceV1::BranchProtection)?;
        }
    }
    if let Some(contexts) = required.get("contexts") {
        let Some(contexts) = contexts.as_array() else {
            return Err("branch protection contexts were malformed");
        };
        for context in contexts {
            let Some(context) = context.as_str() else {
                return Err("branch protection context was malformed");
            };
            push(out, context, None, RequiredSourceV1::BranchProtection)?;
        }
    }
    Ok(())
}

/// Active ruleset rules from `GET repos/{repo}/rules/branches/{branch}`.
pub fn parse_rules(rules: &[Value], out: &mut Vec<RequiredContextV1>) -> Result<(), &'static str> {
    for rule in rules {
        let Some(kind) = rule.get("type").and_then(Value::as_str) else {
            return Err("a ruleset rule had no type");
        };
        if kind != "required_status_checks" {
            continue;
        }
        let Some(checks) = rule
            .get("parameters")
            .and_then(|p| p.get("required_status_checks"))
            .and_then(Value::as_array)
        else {
            return Err("a required-checks rule had no checks");
        };
        for check in checks {
            let Some(context) = check.get("context").and_then(Value::as_str) else {
                return Err("a ruleset required check had no context");
            };
            let app_id = check.get("integration_id").and_then(Value::as_u64);
            push(out, context, app_id, RequiredSourceV1::Ruleset)?;
        }
    }
    Ok(())
}

/// Percent-encodes a branch name for a REST path, keeping `/` (GitHub accepts it).
fn branch_path(base_ref: &str) -> Option<String> {
    if base_ref.is_empty()
        || base_ref.len() > 255
        || base_ref.starts_with('/')
        || base_ref.contains("..")
        || base_ref.chars().any(char::is_control)
    {
        return None;
    }
    let mut out = String::new();
    for byte in base_ref.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    Some(out)
}

fn unavailable_reason(error: &GithubError) -> &'static str {
    match error.kind {
        GithubErrorKind::NotFound => "not visible to this account or not supported by this host",
        GithubErrorKind::Permission => "permission denied",
        GithubErrorKind::RateLimited => "rate limited",
        GithubErrorKind::Json | GithubErrorKind::OutputLimit => "the response could not be read",
        _ => "could not be read",
    }
}

impl GithubProvider {
    /// Reads the required checks of `base_ref`. Account failures and cancellation
    /// propagate (the caller hides private pages); any other read failure is an
    /// explicit `Unavailable` answer rather than an empty required set.
    pub fn required_checks(
        &self,
        repo: &str,
        base_ref: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<RequiredChecksV1> {
        validate_repo(repo)?;
        let unavailable = |reason: &str| {
            Ok(RequiredChecksV1::Unavailable {
                base_ref: base_ref.into(),
                reason: reason.into(),
            })
        };
        let Some(branch) = branch_path(base_ref) else {
            return unavailable("the base branch name cannot be read");
        };
        let propagate = |error: &GithubError| {
            cancel.is_cancelled()
                || matches!(
                    error.kind,
                    GithubErrorKind::Authentication
                        | GithubErrorKind::IdentityChanged
                        | GithubErrorKind::MissingGh
                )
        };
        let mut contexts = Vec::new();
        match self.api(
            &format!("repos/{repo}/branches/{branch}"),
            "GET",
            None,
            None,
            false,
            cancel,
        ) {
            Ok(value) => {
                if let Err(reason) = parse_branch_protection(&value, &mut contexts) {
                    return unavailable(reason);
                }
            }
            Err(error) if propagate(&error) => return Err(error),
            Err(error) => {
                return unavailable(&format!("branch protection {}", unavailable_reason(&error)))
            }
        }
        match self.api_list(
            &format!("repos/{repo}/rules/branches/{branch}?per_page=100"),
            None,
            cancel,
        ) {
            Ok(rules) => {
                if let Err(reason) = parse_rules(&rules, &mut contexts) {
                    return unavailable(reason);
                }
            }
            Err(error) if propagate(&error) => return Err(error),
            Err(error) => return unavailable(&format!("rulesets {}", unavailable_reason(&error))),
        }
        Ok(RequiredChecksV1::Known {
            base_ref: base_ref.into(),
            contexts,
        })
    }
}

#[cfg(test)]
#[path = "required_checks_v1_tests.rs"]
mod tests;
