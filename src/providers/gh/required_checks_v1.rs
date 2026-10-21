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
    /// A result with this name was reported, but the rule pins an app the result
    /// does not name (a commit status), so it is not known to satisfy the rule.
    Unverified,
}

/// One observed result that can satisfy a required context: its name, the app
/// that reported it when known, and the state it contributes.
pub trait ObservedContextV1 {
    fn context_name(&self) -> Option<&str>;
    fn context_app_id(&self) -> Option<u64>;
    fn context_state(&self) -> RequiredStateV1;
}

impl<T: ObservedContextV1 + ?Sized> ObservedContextV1 for &T {
    fn context_name(&self) -> Option<&str> {
        (**self).context_name()
    }
    fn context_app_id(&self) -> Option<u64> {
        (**self).context_app_id()
    }
    fn context_state(&self) -> RequiredStateV1 {
        (**self).context_state()
    }
}

impl ObservedContextV1 for Check {
    fn context_name(&self) -> Option<&str> {
        Some(&self.name)
    }
    fn context_app_id(&self) -> Option<u64> {
        self.app_id
    }
    fn context_state(&self) -> RequiredStateV1 {
        check_state(self)
    }
}

impl ObservedContextV1 for super::ObservedPrCheckV1 {
    fn context_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn context_app_id(&self) -> Option<u64> {
        self.app_id
    }
    fn context_state(&self) -> RequiredStateV1 {
        if !self.current_context_unique {
            // Repeated contexts have no single current result.
            return RequiredStateV1::Pending;
        }
        conclusion_state(self.conclusion.as_deref().unwrap_or(""))
    }
}

/// `Some(true)` when `observed` satisfies `required`, `Some(false)` when it is
/// another context or another app's result, `None` when the rule pins an app
/// the result does not name.
pub fn context_matches<C: ObservedContextV1 + ?Sized>(
    required: &RequiredContextV1,
    observed: &C,
) -> Option<bool> {
    if observed.context_name() != Some(required.context.as_str()) {
        return Some(false);
    }
    match (required.app_id, observed.context_app_id()) {
        (None, _) => Some(true),
        (Some(pinned), Some(app)) => Some(pinned == app),
        (Some(_), None) => None,
    }
}

const MAX_CONTEXTS: usize = 500;

impl RequiredChecksV1 {
    pub fn base_ref(&self) -> &str {
        match self {
            Self::Known { base_ref, .. } | Self::Unavailable { base_ref, .. } => base_ref,
        }
    }
    /// `Some(true)` required, `Some(false)` optional, `None` unknown.
    pub fn required<C: ObservedContextV1 + ?Sized>(&self, check: &C) -> Option<bool> {
        match self {
            Self::Known { contexts, .. } => {
                let mut unknown = false;
                for required in contexts {
                    match context_matches(required, check) {
                        Some(true) => return Some(true),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                (!unknown).then_some(false)
            }
            Self::Unavailable { .. } => None,
        }
    }
    /// Required contexts and their state on the observed checks, in rule order.
    pub fn states<'a, C: ObservedContextV1>(
        &'a self,
        checks: &[C],
    ) -> Vec<(&'a str, RequiredStateV1)> {
        let Self::Known { contexts, .. } = self else {
            return Vec::new();
        };
        contexts
            .iter()
            .map(|required| {
                let mut state = RequiredStateV1::NotReported;
                let mut unverified = false;
                for check in checks {
                    match context_matches(required, check) {
                        Some(true) => {}
                        None => {
                            unverified = true;
                            continue;
                        }
                        Some(false) => continue,
                    }
                    let this = check.context_state();
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
                if state == RequiredStateV1::NotReported && unverified {
                    state = RequiredStateV1::Unverified;
                }
                (required.context.as_str(), state)
            })
            .collect()
    }
    /// One line for the Checks tab header, with the role that colours it.
    pub fn summary<C: ObservedContextV1>(
        &self,
        checks: &[C],
    ) -> (String, crate::theme::SemanticToken) {
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
                let (failing, pending, passed, missing, unverified) = (
                    count(RequiredStateV1::Failing),
                    count(RequiredStateV1::Pending),
                    count(RequiredStateV1::Passed),
                    count(RequiredStateV1::NotReported),
                    count(RequiredStateV1::Unverified),
                );
                let parts: Vec<String> = [
                    (failing, "failing"),
                    (pending, "pending"),
                    (missing, "not reported"),
                    (unverified, "app unverified"),
                    (passed, "passed"),
                ]
                .into_iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, word)| format!("{n} {word}"))
                .collect();
                let role = if failing > 0 {
                    SemanticToken::Failure
                } else if pending + missing + unverified > 0 {
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
    conclusion_state(&check.conclusion)
}

fn conclusion_state(conclusion: &str) -> RequiredStateV1 {
    match conclusion {
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

/// Reads the required checks of `base_ref` through `branch` (classic protection
/// for an encoded branch path) and `rules` (active rulesets). Account failures
/// and cancellation propagate (the caller hides private pages); any other read
/// failure is an explicit `Unavailable` answer rather than an empty required set.
pub(crate) fn read_required_checks_v1(
    base_ref: &str,
    cancel: &CancellationToken,
    branch: impl FnOnce(&str) -> GithubResult<Value>,
    rules: impl FnOnce(&str) -> GithubResult<Vec<Value>>,
) -> GithubResult<RequiredChecksV1> {
    let unavailable = |reason: &str| {
        Ok(RequiredChecksV1::Unavailable {
            base_ref: base_ref.into(),
            reason: reason.into(),
        })
    };
    let Some(path) = branch_path(base_ref) else {
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
    match branch(&path) {
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
    match rules(&path) {
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

impl GithubProvider {
    /// Reads the required checks of `base_ref`; see [`read_required_checks_v1`].
    pub fn required_checks(
        &self,
        repo: &str,
        base_ref: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<RequiredChecksV1> {
        validate_repo(repo)?;
        read_required_checks_v1(
            base_ref,
            cancel,
            |branch| {
                self.api(
                    &format!("repos/{repo}/branches/{branch}"),
                    "GET",
                    None,
                    None,
                    false,
                    cancel,
                )
            },
            |branch| {
                self.api_list(
                    &format!("repos/{repo}/rules/branches/{branch}?per_page=100"),
                    None,
                    cancel,
                )
            },
        )
    }
}

#[cfg(test)]
#[path = "required_checks_v1_tests.rs"]
mod tests;
