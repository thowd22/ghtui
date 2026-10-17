//! Pure local syntax refusal, never an authentication or publication permit.
//! Callers supply their actual fresh identity bracket, not cached UI metadata.
use super::*;

impl GithubProvider {
    pub(crate) fn require_cli_syntax_v1(
        &self,
        current: &HostIdentity,
        requirement: CliRequirement,
    ) -> GithubResult<()> {
        if current.host != self.host {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "CLI preflight belongs to another host",
            ));
        }
        let version = GhCliVersion::parse_line(&current.gh_version).ok();
        let syntax = requirement.assess(version.as_ref());
        if matches!(syntax, LocalCliSyntax::KnownDocumentedSyntax { .. }) {
            return Ok(());
        }
        let floor = requirement.documented_floor().map_or_else(
            || "with reviewed option support".into(),
            |floor| floor.to_string(),
        );
        let observed = version
            .as_ref()
            .map_or_else(|| "unverified".into(), |v| v.numbers().to_string());
        // Only bounded parsed numbers and closed requirement text are echoed.
        // A malformed version, build annotation or secret value is never copied.
        Err(error(
            GithubErrorKind::Unsupported,
            format!("gh {floor} required for {}. Current version: {observed}. {}. Use a reviewed stable build, refresh and preview again. Exact options remain required; no secret invocation started.", requirement.exact_flags().join(" "), syntax.limitation()),
        ))
    }
}
