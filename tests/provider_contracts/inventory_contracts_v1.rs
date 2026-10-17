use super::*;
use ghtui::providers::gh::CapabilityEffectV1;

#[test]
fn gh_provider_inventory_contracts_provider_lookup_is_local_not_a_permission_or_host_probe() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let inventory = provider(&fixture).capabilities();
    assert_eq!(inventory.len(), 33);
    for c in &inventory {
        let requirements = c.requirements_v1().expect("mapped builtin");
        assert!(!requirements.rows.is_empty());
        assert!(requirements.host_limit().contains("unverified"));
    }
    let read = inventory.iter().find(|c| c.id == "actions-logs").unwrap();
    let write = inventory
        .iter()
        .find(|c| c.id == "actions-secrets-variables")
        .unwrap();
    assert_eq!(
        read.requirements_v1().unwrap().effect(),
        CapabilityEffectV1::ReadOnly
    );
    assert_eq!(
        write.requirements_v1().unwrap().effect(),
        CapabilityEffectV1::RemoteWrite
    );
    assert!(write
        .requirements_v1()
        .unwrap()
        .operations
        .contains("Existing secret values cannot be read"));
    assert!(
        journal(&fixture).is_empty(),
        "Constructing requirements must not start gh, inspect auth or probe a host"
    );
}
