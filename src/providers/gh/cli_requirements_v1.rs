//! SOURCE ONLY: undeclared local CLI syntax facts, not host/permission authority.
//! No probes, authentication, I/O, registration framework, global security
//! baseline or mutation fallback. Callers select their ACTUAL argv family.
use std::fmt;

const MAX_VERSION_LINE: usize = 256;
const MAX_VERSION_STDOUT: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GhVersionNumbers {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}
impl fmt::Display for GhVersionNumbers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhVersionParseError {
    OutputBudget,
    InvalidUtf8,
    UnsafeOutput,
    InvalidPrefix,
    InvalidComponents,
    InvalidSuffix,
}
impl fmt::Display for GhVersionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutputBudget => "gh version output exceeds its bounded local budget",
            Self::InvalidUtf8 => "gh version output is not valid UTF-8",
            Self::UnsafeOutput => "gh version output contains unsupported bytes or controls",
            Self::InvalidPrefix => "Expected an exact gh version line, not a compatibility guess",
            Self::InvalidComponents => "Expected three canonical numeric gh version components",
            Self::InvalidSuffix => "gh version build/date suffix is malformed",
        })
    }
}
impl std::error::Error for GhVersionParseError {}

/// Private parsed fields prevent a fabricated unbounded/custom release from
/// becoming a stable local syntax observation. It is not serialized authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhCliVersion {
    numbers: GhVersionNumbers,
    build_suffix: Option<String>,
    annotation: Option<String>,
    stable_release_shape: bool,
}
impl GhCliVersion {
    /// Consume raw version stdout BEFORE lossy conversion/sanitation. A normal
    /// second release-URL line is allowed, but never interpreted as a feature.
    pub fn parse_stdout(raw: &[u8]) -> Result<Self, GhVersionParseError> {
        if raw.is_empty() || raw.len() > MAX_VERSION_STDOUT {
            return Err(GhVersionParseError::OutputBudget);
        }
        let text = std::str::from_utf8(raw).map_err(|_| GhVersionParseError::InvalidUtf8)?;
        if text.bytes().enumerate().any(|(index, b)| {
            !(b.is_ascii_graphic() || matches!(b, b' ' | b'\n' | b'\r'))
                || b == b'\r' && raw.get(index + 1) != Some(&b'\n')
        }) {
            return Err(GhVersionParseError::UnsafeOutput);
        }
        let first = text.split_once('\n').map_or(text, |(first, _)| first);
        Self::parse_line(first.strip_suffix('\r').unwrap_or(first))
    }

    pub fn parse_line(line: &str) -> Result<Self, GhVersionParseError> {
        if line.is_empty() || line.len() > MAX_VERSION_LINE {
            return Err(GhVersionParseError::OutputBudget);
        }
        if line.bytes().any(|b| !(b.is_ascii_graphic() || b == b' ')) {
            return Err(GhVersionParseError::UnsafeOutput);
        }
        let version = line
            .strip_prefix("gh version ")
            .ok_or(GhVersionParseError::InvalidPrefix)?;
        let (token, annotation) = match version.split_once(' ') {
            Some((token, annotation)) => {
                let annotation = annotation
                    .strip_prefix('(')
                    .and_then(|value| value.strip_suffix(')'))
                    .filter(|value| {
                        !value.is_empty()
                            && !value.starts_with(' ')
                            && !value.ends_with(' ')
                            && value.bytes().all(|b| {
                                b.is_ascii_alphanumeric()
                                    || matches!(b, b' ' | b'.' | b'-' | b'_' | b'/' | b':')
                            })
                    })
                    .ok_or(GhVersionParseError::InvalidSuffix)?;
                (token, Some(annotation))
            }
            None => (version, None),
        };
        let split = token.find(['-', '+']).unwrap_or(token.len());
        let (core, suffix) = token.split_at(split);
        let mut parts = core.split('.');
        let numbers = GhVersionNumbers {
            major: component(parts.next())?,
            minor: component(parts.next())?,
            patch: component(parts.next())?,
        };
        if numbers.major == 0 || parts.next().is_some() {
            return Err(GhVersionParseError::InvalidComponents);
        }
        if !suffix.is_empty() && !valid_build_suffix(suffix) {
            return Err(GhVersionParseError::InvalidSuffix);
        }
        let annotation_is_date = annotation.is_none_or(valid_date);
        Ok(Self {
            numbers,
            build_suffix: (!suffix.is_empty()).then(|| suffix.to_owned()),
            annotation: annotation.map(str::to_owned),
            stable_release_shape: suffix.is_empty() && annotation_is_date,
        })
    }
    pub fn numbers(&self) -> GhVersionNumbers {
        self.numbers
    }
    pub fn build_suffix(&self) -> Option<&str> {
        self.build_suffix.as_deref()
    }
    pub fn annotation(&self) -> Option<&str> {
        self.annotation.as_deref()
    }
    pub fn stable_release_shape(&self) -> bool {
        self.stable_release_shape
    }
}
fn component(value: Option<&str>) -> Result<u32, GhVersionParseError> {
    value
        .filter(|v| {
            !v.is_empty()
                && (v.len() == 1 || !v.starts_with('0'))
                && v.bytes().all(|b| b.is_ascii_digit())
        })
        .and_then(|v| v.parse().ok())
        .ok_or(GhVersionParseError::InvalidComponents)
}
fn valid_build_suffix(value: &str) -> bool {
    let value = &value[1..];
    let (prerelease, metadata) = match value.split_once('+') {
        Some((pre, metadata)) if !metadata.contains('+') => (pre, Some(metadata)),
        Some(_) => return false,
        None => (value, None),
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part.split('.').all(|id| {
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
    };
    valid(prerelease) && metadata.is_none_or(valid)
}
fn valid_date(value: &str) -> bool {
    if value.len() != 10 || &value[4..5] != "-" || &value[7..8] != "-" {
        return false;
    }
    let number = |range: std::ops::Range<usize>| {
        let text = &value[range];
        text.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| text.parse::<u32>().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day)
}

/// Closed ACTUAL argv families; metadata's fixed-page/API-input paths are not
/// hypothetical `gh pr edit` and do not inherit the common --slurp requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActualGhTransport {
    FixedPageApiJson,
    GraphqlApiInput,
    CommonApiPaginateSlurp,
    PrMergeMatchHeadCommit,
    PrReadyUndo,
    RunViewAttemptLogs,
    SecretSetNoReposSelected,
    ApiRawAllowEscapeSequences,
    PrDiffAllowEscapeSequences,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliRequirement {
    NoAdditionalDocumentedFlag,
    ApiSlurp,
    MergeExpectedHead,
    ReadyUndo,
    RunAttempt,
    SecretEmptySelection,
    RawEscapeOptOut,
}
impl ActualGhTransport {
    pub fn requirement(self) -> CliRequirement {
        match self {
            Self::FixedPageApiJson | Self::GraphqlApiInput => {
                CliRequirement::NoAdditionalDocumentedFlag
            }
            Self::CommonApiPaginateSlurp => CliRequirement::ApiSlurp,
            Self::PrMergeMatchHeadCommit => CliRequirement::MergeExpectedHead,
            Self::PrReadyUndo => CliRequirement::ReadyUndo,
            Self::RunViewAttemptLogs => CliRequirement::RunAttempt,
            Self::SecretSetNoReposSelected => CliRequirement::SecretEmptySelection,
            Self::ApiRawAllowEscapeSequences | Self::PrDiffAllowEscapeSequences => {
                CliRequirement::RawEscapeOptOut
            }
        }
    }
}
impl CliRequirement {
    pub fn documented_floor(self) -> Option<GhVersionNumbers> {
        let minor = match self {
            Self::NoAdditionalDocumentedFlag => return None,
            Self::ApiSlurp => 48,
            Self::MergeExpectedHead => 13,
            Self::ReadyUndo => 16,
            Self::RunAttempt => 26,
            Self::SecretEmptySelection => 75,
            Self::RawEscapeOptOut => 97,
        };
        Some(GhVersionNumbers {
            major: 2,
            minor,
            patch: 0,
        })
    }
    pub fn exact_flags(self) -> &'static [&'static str] {
        match self {
            Self::NoAdditionalDocumentedFlag => &[],
            Self::ApiSlurp => &["--paginate", "--slurp"],
            Self::MergeExpectedHead => &["--match-head-commit"],
            Self::ReadyUndo => &["--undo"],
            Self::RunAttempt => &["--attempt"],
            Self::SecretEmptySelection => &["--no-repos-selected"],
            Self::RawEscapeOptOut => &["--allow-escape-sequences"],
        }
    }
    pub fn source_url(self) -> Option<&'static str> {
        Some(match self {
            Self::NoAdditionalDocumentedFlag => return None,
            Self::ApiSlurp => "https://github.com/cli/cli/releases/tag/v2.48.0",
            Self::MergeExpectedHead => "https://github.com/cli/cli/releases/tag/v2.13.0",
            Self::ReadyUndo => "https://github.com/cli/cli/releases/tag/v2.16.0",
            Self::RunAttempt => "https://github.com/cli/cli/releases/tag/v2.26.0",
            Self::SecretEmptySelection => "https://github.com/cli/cli/releases/tag/v2.75.0",
            Self::RawEscapeOptOut => "https://github.com/cli/cli/releases/tag/v2.97.0",
        })
    }
    pub fn assess(self, version: Option<&GhCliVersion>) -> LocalCliSyntax {
        let Some(version) = version else {
            return LocalCliSyntax::Unverified(CliUnverified::VersionUnavailable);
        };
        if version.numbers.major > 2 {
            return LocalCliSyntax::Unverified(CliUnverified::FutureMajor);
        }
        if version.numbers.major < 2 {
            return LocalCliSyntax::OutsideAuditedMajor;
        }
        if !version.stable_release_shape {
            return LocalCliSyntax::Unverified(CliUnverified::CustomOrPrereleaseBuild);
        }
        let Some(floor) = self.documented_floor() else {
            return LocalCliSyntax::NoAdditionalDocumentedFlagFloor;
        };
        if version.numbers < floor {
            LocalCliSyntax::BelowDocumentedFloor { floor }
        } else {
            LocalCliSyntax::KnownDocumentedSyntax { floor }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliUnverified {
    VersionUnavailable,
    FutureMajor,
    CustomOrPrereleaseBuild,
}
/// This outcome describes LOCAL FLAG SYNTAX ONLY. It cannot carry host/Scope,
/// user permissions, command payload, receipt, native completion or approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalCliSyntax {
    KnownDocumentedSyntax { floor: GhVersionNumbers },
    BelowDocumentedFloor { floor: GhVersionNumbers },
    NoAdditionalDocumentedFlagFloor,
    OutsideAuditedMajor,
    Unverified(CliUnverified),
}
impl LocalCliSyntax {
    pub fn limitation(self) -> &'static str {
        match self {
            Self::KnownDocumentedSyntax { .. } => "Documented local flag syntax only; host support, current permissions, security baseline and native delivery remain independent",
            Self::BelowDocumentedFloor { .. } => "Installed gh predates the documented required flag; upgrade without dropping exact head/attempt/scope or retrying a write",
            Self::NoAdditionalDocumentedFlagFloor => "This fixed API transport does not use the additional audited flags; basic API compatibility, security policy, host and permissions are not verified here",
            Self::OutsideAuditedMajor => "This major is outside the reviewed gh 2.x syntax; install a reviewed build, not an unspecified newer version",
            Self::Unverified(CliUnverified::VersionUnavailable) => "No strictly parsed local gh version; inspect or install a reviewed build",
            Self::Unverified(CliUnverified::FutureMajor) => "Future gh major is unaudited, not implicitly compatible because its number is higher",
            Self::Unverified(CliUnverified::CustomOrPrereleaseBuild) => "Custom/prerelease build retains its suffix; flag support is unverified without selected local evidence",
        }
    }
}

#[cfg(test)]
#[path = "cli_requirements_v1_tests.rs"]
mod tests;
