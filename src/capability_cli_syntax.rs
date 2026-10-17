//! Local syntax explanations for exact provider routes, not host probes,
//! operation eligibility, native parity, or a mutation capability registry.
use crate::providers::gh::{
    capability_requirement_contract_v1, CliRequirement, CliUnverified, GhCliVersion, LocalCliSyntax,
};

fn fact(id: &str) -> Option<(&'static str, CliRequirement)> {
    capability_requirement_contract_v1(id)?.cli
}

pub(super) fn details(id: &str, version: Option<&GhCliVersion>) -> Vec<String> {
    let Some((route, requirement)) = fact(id) else {
        return vec!["CLI syntax: no exact operation-flag floor reviewed for this category; do not infer one from its transport label.".into()];
    };
    let floor = requirement
        .documented_floor()
        .expect("closed documented flag facts");
    let state = match requirement.assess(version) {
        LocalCliSyntax::KnownDocumentedSyntax { .. } => {
            "numeric release shape meets documented flag floor"
        }
        LocalCliSyntax::BelowDocumentedFloor { .. } => {
            "below documented flag floor; upgrade gh before using this CLI route"
        }
        LocalCliSyntax::Unverified(CliUnverified::CustomOrPrereleaseBuild) => {
            "custom/prerelease build; flag support unverified"
        }
        LocalCliSyntax::Unverified(CliUnverified::FutureMajor) => {
            "future major; flag support unverified"
        }
        LocalCliSyntax::Unverified(CliUnverified::VersionUnavailable) => {
            "no parsed last-observed version; flag support unverified"
        }
        LocalCliSyntax::OutsideAuditedMajor => {
            "major outside reviewed gh 2.x syntax; flag support unverified"
        }
        LocalCliSyntax::NoAdditionalDocumentedFlagFloor => {
            unreachable!("each closed fact has an additional flag")
        }
    };
    vec![
        format!("CLI provider route only: {route}. Other native routes in this category are separate."),
        format!("Documented local flag floor: gh {floor}; exact flags: {}.", requirement.exact_flags().join(" ")),
        format!("Last-observed CLI syntax: {state}. This is not a fresh executable probe."),
        "Host/API support, current target permissions, dependency security and native workflow coverage remain unverified here. Never drop an exact head, attempt or scope flag to gain compatibility.".into(),
        format!("CLI source: {}", requirement.source_url().expect("documented flag source")),
    ]
}
