//! SOURCE ONLY, ten undeclared/unrun pure local-syntax contracts.
use super::*;

fn parsed(version: &str) -> GhCliVersion {
    GhCliVersion::parse_line(&format!("gh version {version}")).unwrap()
}

#[test]
fn gh_provider_cli_version_stable_components_and_release_dates_are_exact() {
    for text in [
        "2.0.0",
        "2.48.0",
        "2.101.0",
        "2.101.0 (2026-09-15)",
        "2.48.1 (2024-02-29)",
    ] {
        let value = parsed(text);
        assert!(value.stable_release_shape());
        assert_eq!(value.numbers.major, 2);
        assert!(value.build_suffix().is_none());
    }
    assert_eq!(
        parsed("2.0.0").numbers(),
        GhVersionNumbers {
            major: 2,
            minor: 0,
            patch: 0
        }
    );
    assert_eq!(
        parsed("2.101.0 (2026-09-15)").annotation(),
        Some("2026-09-15")
    );
}

#[test]
fn gh_provider_cli_version_raw_stdout_accepts_bounded_url_line_not_lossy_utf8() {
    let text =
        b"gh version 2.48.0 (2024-04-17)\r\nhttps://github.com/cli/cli/releases/tag/v2.48.0\r\n";
    assert_eq!(
        GhCliVersion::parse_stdout(text).unwrap(),
        parsed("2.48.0 (2024-04-17)")
    );
    assert_eq!(
        GhCliVersion::parse_stdout(b"gh version 2.48.0\n\xff"),
        Err(GhVersionParseError::InvalidUtf8)
    );
    assert_eq!(
        GhCliVersion::parse_stdout(b"gh version 2.48.0\n\x1b[31m"),
        Err(GhVersionParseError::UnsafeOutput)
    );
    assert_eq!(
        GhCliVersion::parse_line("gh version 2.48.0\r"),
        Err(GhVersionParseError::UnsafeOutput)
    );
    assert_eq!(
        GhCliVersion::parse_stdout(b"gh version 2.48.0\r"),
        Err(GhVersionParseError::UnsafeOutput)
    );
}

#[test]
fn gh_provider_cli_version_malformed_numeric_and_prefix_claims_never_parse() {
    for text in [
        "2.",
        "2.48",
        "2.48.0.1",
        "2.+48.0",
        "2.48.-1",
        "2.048.0",
        "02.48.0",
        "2.48.00",
        "0.48.0",
        "4294967296.48.0",
        "2.4294967296.0",
        "2.48.4294967296",
    ] {
        assert!(
            GhCliVersion::parse_line(&format!("gh version {text}")).is_err(),
            "{text}"
        );
    }
    for text in [
        "gh version2.48.0",
        "gh version  2.48.0",
        "GH version 2.48.0",
        "git version 2.48.0",
        " gh version 2.48.0",
        "gh version v2.48.0",
    ] {
        assert!(GhCliVersion::parse_line(text).is_err(), "{text}");
    }
}

#[test]
fn gh_provider_cli_version_preserves_distro_prerelease_without_approving_floor() {
    for text in [
        "2.101.0-rc.1",
        "2.101.0+dfsg1",
        "2.101.0-1ubuntu0.3",
        "2.101.0-rc.1+debian.2",
        "2.101.0 (Ubuntu 2026.10)",
        "2.101.0 (2025-02-29)",
    ] {
        let version = parsed(text);
        assert!(!version.stable_release_shape(), "{text}");
        assert_eq!(
            CliRequirement::ApiSlurp.assess(Some(&version)),
            LocalCliSyntax::Unverified(CliUnverified::CustomOrPrereleaseBuild)
        );
        assert!(version.build_suffix().is_some() || version.annotation().is_some());
    }
    assert_eq!(
        parsed("2.101.0-1ubuntu0.3").build_suffix(),
        Some("-1ubuntu0.3")
    );
    for text in [
        "2.101.0-",
        "2.101.0+",
        "2.101.0-rc..1",
        "2.101.0++debian",
        "2.101.0 (bad) trailing",
        "2.101.0 (nested (date))",
        "2.101.0 ",
    ] {
        assert!(
            GhCliVersion::parse_line(&format!("gh version {text}")).is_err(),
            "{text}"
        );
    }
}

#[test]
fn gh_provider_cli_version_budget_controls_and_nonascii_refuse_before_retention() {
    for byte in [0_u8, 7, 8, 9, 13, 27, 127] {
        let raw = format!("gh version 2.48.0{}", char::from(byte));
        assert!(GhCliVersion::parse_line(&raw).is_err());
    }
    for text in [
        "gh version ２.48.0",
        "gh version 2.48.0 (build\u{202e})",
        "gh version 2.48.0-β",
    ] {
        assert_eq!(
            GhCliVersion::parse_line(text),
            Err(GhVersionParseError::UnsafeOutput)
        );
    }
    assert_eq!(
        GhCliVersion::parse_line(&"x".repeat(257)),
        Err(GhVersionParseError::OutputBudget)
    );
    assert_eq!(
        GhCliVersion::parse_stdout(&vec![b'x'; 4097]),
        Err(GhVersionParseError::OutputBudget)
    );
    assert!(!GhVersionParseError::InvalidComponents
        .to_string()
        .contains("secret-canary"));
}

#[test]
fn gh_provider_cli_requirements_every_documented_floor_has_exact_boundaries() {
    for (requirement, minor) in [
        (CliRequirement::MergeExpectedHead, 13),
        (CliRequirement::ReadyUndo, 16),
        (CliRequirement::RunAttempt, 26),
        (CliRequirement::ApiSlurp, 48),
        (CliRequirement::SecretEmptySelection, 75),
        (CliRequirement::RawEscapeOptOut, 97),
    ] {
        let floor = GhVersionNumbers {
            major: 2,
            minor,
            patch: 0,
        };
        assert_eq!(requirement.documented_floor(), Some(floor));
        assert_eq!(
            requirement.assess(Some(&parsed(&format!("2.{}.99", minor - 1)))),
            LocalCliSyntax::BelowDocumentedFloor { floor }
        );
        assert_eq!(
            requirement.assess(Some(&parsed(&format!("2.{minor}.0")))),
            LocalCliSyntax::KnownDocumentedSyntax { floor }
        );
        assert_eq!(
            requirement.assess(Some(&parsed("2.101.0"))),
            LocalCliSyntax::KnownDocumentedSyntax { floor }
        );
        assert_eq!(
            requirement.source_url(),
            Some(format!("https://github.com/cli/cli/releases/tag/v2.{minor}.0").as_str())
        );
    }
}

#[test]
fn gh_provider_cli_requirements_unknown_future_major_is_not_newer_means_supported() {
    for requirement in [
        CliRequirement::ApiSlurp,
        CliRequirement::MergeExpectedHead,
        CliRequirement::NoAdditionalDocumentedFlag,
    ] {
        assert_eq!(
            requirement.assess(None),
            LocalCliSyntax::Unverified(CliUnverified::VersionUnavailable)
        );
        assert_eq!(
            requirement.assess(Some(&parsed("3.0.0"))),
            LocalCliSyntax::Unverified(CliUnverified::FutureMajor)
        );
        assert_eq!(
            requirement.assess(Some(&parsed("4294967295.0.0"))),
            LocalCliSyntax::Unverified(CliUnverified::FutureMajor)
        );
        assert_eq!(
            requirement.assess(Some(&parsed("1.999.999"))),
            LocalCliSyntax::OutsideAuditedMajor
        );
    }
    assert!(LocalCliSyntax::Unverified(CliUnverified::FutureMajor)
        .limitation()
        .contains("unaudited"));
}

#[test]
fn gh_provider_cli_requirements_fixed_metadata_transport_does_not_inherit_slurp_or_edit_floor() {
    for transport in [
        ActualGhTransport::FixedPageApiJson,
        ActualGhTransport::GraphqlApiInput,
    ] {
        let requirement = transport.requirement();
        assert!(requirement.documented_floor().is_none());
        assert!(requirement.exact_flags().is_empty());
        assert_eq!(
            requirement.assess(Some(&parsed("2.47.0"))),
            LocalCliSyntax::NoAdditionalDocumentedFlagFloor
        );
        assert!(requirement
            .assess(Some(&parsed("2.101.0")))
            .limitation()
            .contains("not verified here"));
    }
    assert_eq!(
        ActualGhTransport::CommonApiPaginateSlurp.requirement(),
        CliRequirement::ApiSlurp
    );
    assert!(matches!(
        CliRequirement::ApiSlurp.assess(Some(&parsed("2.47.0"))),
        LocalCliSyntax::BelowDocumentedFloor { .. }
    ));
}

#[test]
fn gh_provider_cli_requirements_actual_command_flags_are_exact_not_hypothetical_names() {
    for (transport, flags) in [
        (
            ActualGhTransport::CommonApiPaginateSlurp,
            &["--paginate", "--slurp"][..],
        ),
        (
            ActualGhTransport::PrMergeMatchHeadCommit,
            &["--match-head-commit"][..],
        ),
        (ActualGhTransport::PrReadyUndo, &["--undo"][..]),
        (ActualGhTransport::RunViewAttemptLogs, &["--attempt"][..]),
        (
            ActualGhTransport::SecretSetNoReposSelected,
            &["--no-repos-selected"][..],
        ),
        (
            ActualGhTransport::ApiRawAllowEscapeSequences,
            &["--allow-escape-sequences"][..],
        ),
        (
            ActualGhTransport::PrDiffAllowEscapeSequences,
            &["--allow-escape-sequences"][..],
        ),
    ] {
        assert_eq!(transport.requirement().exact_flags(), flags);
        for misleading in [
            "--slurpish",
            "--match-head",
            "--attempts",
            "--no-repos-selected-ish",
        ] {
            assert!(!transport.requirement().exact_flags().contains(&misleading));
        }
    }
}

#[test]
fn gh_provider_cli_requirements_syntax_facts_do_not_grant_permissions_or_security_policy() {
    let old = parsed("2.48.0");
    assert!(matches!(
        CliRequirement::ApiSlurp.assess(Some(&old)),
        LocalCliSyntax::KnownDocumentedSyntax { .. }
    ));
    assert!(matches!(
        CliRequirement::SecretEmptySelection.assess(Some(&old)),
        LocalCliSyntax::BelowDocumentedFloor { .. }
    ));
    assert!(matches!(
        CliRequirement::RawEscapeOptOut.assess(Some(&old)),
        LocalCliSyntax::BelowDocumentedFloor { .. }
    ));
    let label = CliRequirement::ApiSlurp.assess(Some(&old)).limitation();
    assert!(
        label.contains("host support")
            && label.contains("permissions")
            && label.contains("security baseline")
            && label.contains("native delivery")
    );
    assert!(!label.contains("allowed to publish"));
}
