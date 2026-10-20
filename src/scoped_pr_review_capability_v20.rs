//! Review dismissal and reviewer re-request capability, derived only from the
//! repository permission flags the review source session already observed
//! (`GET repos/{owner}/{repo}`). No extra request is made. A missing flag is
//! never read as a denial: only a complete, verified set of flags can disable
//! a control, and only a verified grant can drop the UNKNOWN label. The server
//! still enforces branch-protection dismissal restrictions after a grant.
use crate::providers::gh::RepositoryCataloguePermissions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewCapabilityV20 {
    /// At least one sufficient role flag was observed as `true`.
    Verified,
    /// Every sufficient role flag was observed as `false`.
    Denied,
    /// Some sufficient flag was missing or malformed; the server decides.
    Unknown,
}

fn any_of(flags: &[Option<bool>]) -> ReviewCapabilityV20 {
    if flags.contains(&Some(true)) {
        ReviewCapabilityV20::Verified
    } else if flags.iter().all(|flag| *flag == Some(false)) {
        ReviewCapabilityV20::Denied
    } else {
        ReviewCapabilityV20::Unknown
    }
}

/// GitHub lets people with write, maintain or admin access dismiss reviews.
pub fn dismiss(permissions: &RepositoryCataloguePermissions) -> ReviewCapabilityV20 {
    any_of(&[permissions.push, permissions.maintain, permissions.admin])
}

/// Requesting reviewers needs triage or above. The pull request's own author
/// may still re-request a reviewer without that role, so a denial for the
/// author stays UNKNOWN and is left to the server.
pub fn request_reviewers(
    permissions: &RepositoryCataloguePermissions,
    viewer_is_author: bool,
) -> ReviewCapabilityV20 {
    match any_of(&[
        permissions.triage,
        permissions.push,
        permissions.maintain,
        permissions.admin,
    ]) {
        ReviewCapabilityV20::Denied if viewer_is_author => ReviewCapabilityV20::Unknown,
        other => other,
    }
}

pub const DISMISS_DENIED_V20: &str =
    "Verified repository permission is read-only; dismissal needs write";
pub const REQUEST_DENIED_V20: &str =
    "Verified repository permission is below triage; re-request refused";

#[cfg(test)]
mod tests {
    use super::*;
    use ReviewCapabilityV20::*;

    fn flags(
        pull: Option<bool>,
        triage: Option<bool>,
        push: Option<bool>,
        maintain: Option<bool>,
        admin: Option<bool>,
    ) -> RepositoryCataloguePermissions {
        RepositoryCataloguePermissions {
            pull,
            push,
            maintain,
            admin,
            triage,
        }
    }

    #[test]
    fn review_capability_v20_dismiss_needs_a_verified_write_role() {
        let t = Some(true);
        let f = Some(false);
        assert_eq!(dismiss(&flags(t, f, t, f, f)), Verified);
        assert_eq!(dismiss(&flags(t, f, f, t, f)), Verified);
        assert_eq!(dismiss(&flags(t, f, f, f, t)), Verified);
        // Triage alone cannot dismiss.
        assert_eq!(dismiss(&flags(t, t, f, f, f)), Denied);
        assert_eq!(dismiss(&flags(t, f, f, f, f)), Denied);
        // A missing flag is never a denial.
        assert_eq!(dismiss(&flags(t, None, f, None, None)), Unknown);
        assert_eq!(dismiss(&RepositoryCataloguePermissions::default()), Unknown);
        // A grant is enough even when other flags are unreported.
        assert_eq!(dismiss(&flags(None, None, t, None, None)), Verified);
    }

    #[test]
    fn review_capability_v20_request_needs_triage_and_spares_the_author() {
        let t = Some(true);
        let f = Some(false);
        assert_eq!(request_reviewers(&flags(t, t, f, f, f), false), Verified);
        assert_eq!(request_reviewers(&flags(t, f, t, f, f), false), Verified);
        assert_eq!(request_reviewers(&flags(t, f, f, f, f), false), Denied);
        assert_eq!(request_reviewers(&flags(t, f, f, f, f), true), Unknown);
        assert_eq!(request_reviewers(&flags(t, None, f, f, f), false), Unknown);
        assert_eq!(
            request_reviewers(&RepositoryCataloguePermissions::default(), false),
            Unknown
        );
    }
}
