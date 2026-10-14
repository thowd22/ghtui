//! Meaningful bounded discovery request-intent contracts.
use super::*;

#[test]
fn repo_catalog_read_limits_defaults_preserve_actual_transport_and_maximum_is_not_coverage() {
    let budget = CatalogueDiscoveryBudgetV1::default();
    let actual = budget.limits();
    let old = RepositoryCatalogueLimits::default();
    assert_eq!(actual.max_pages, old.max_pages);
    assert_eq!(actual.page_size, old.page_size);
    assert_eq!(actual.max_rows, old.max_rows);
    assert_eq!(actual.max_metadata_bytes, old.max_metadata_bytes);
    assert_eq!(actual.max_response_bytes, old.max_response_bytes);
    assert_eq!(actual.max_duration, old.max_duration);
    let maximum = CatalogueDiscoveryBudgetV1::preset(2).unwrap().limits();
    assert_eq!(maximum.max_pages, 1000);
    assert_eq!(maximum.max_rows, 100000);
    assert_eq!(maximum.max_metadata_bytes, 32 * 1024 * 1024);
    assert_eq!(maximum.max_response_bytes, 64 * 1024 * 1024);
    assert_eq!(maximum.max_duration, Duration::from_secs(300));
    assert_eq!(maximum.page_size, 100);
    assert_eq!(maximum.max_document_bytes, 128 * 1024);
}

#[test]
fn repo_catalog_read_limits_numeric_raw_preflight_rejects_controls_unicode_large_and_missing() {
    for bad in [
        "",
        "1000000",
        "-1",
        "+1",
        "1.0",
        "1e2",
        " 20",
        "20\n",
        "２０",
        "\u{202e}20",
        "\u{1b}[0m",
    ] {
        for index in 0..DISCOVERY_FIELD_COUNT {
            let mut fields = ["20", "2000", "8", "16", "30"];
            fields[index] = bad;
            let error = CatalogueDiscoveryBudgetV1::from_fields(fields).unwrap_err();
            assert_eq!(
                error,
                "Use 1..6 ASCII decimal digits per field; no request sent"
            );
            assert!(!error.contains(bad) || bad.is_empty());
        }
    }
}

#[test]
fn repo_catalog_read_limits_each_closed_range_refuses_no_clamping_and_allows_short_clock() {
    for (index, below, above) in [
        (0, "0", "1001"),
        (1, "99", "100001"),
        (2, "0", "33"),
        (3, "0", "65"),
        (4, "0", "301"),
    ] {
        for invalid in [below, above] {
            let mut fields = ["20", "2000", "8", "16", "30"];
            fields[index] = invalid;
            assert!(CatalogueDiscoveryBudgetV1::from_fields(fields).is_err());
        }
    }
    let minimum = CatalogueDiscoveryBudgetV1::from_fields(["1", "100", "1", "1", "1"])
        .unwrap()
        .limits();
    assert_eq!(minimum.max_pages, 1);
    assert_eq!(minimum.max_rows, 100);
    assert_eq!(minimum.max_duration, Duration::from_secs(1));
}

#[test]
fn repo_catalog_read_limits_larger_preview_never_reduces_custom_bound_or_suggests_unbounded() {
    let normal = CatalogueDiscoveryBudgetV1::default().suggested_larger();
    assert_eq!(normal, CatalogueDiscoveryBudgetV1::preset(1).unwrap());
    let custom =
        CatalogueDiscoveryBudgetV1::from_fields(["50", "70000", "24", "16", "240"]).unwrap();
    let suggested = custom.suggested_larger().limits();
    assert_eq!(suggested.max_pages, 100);
    assert_eq!(suggested.max_rows, 70000);
    assert_eq!(suggested.max_metadata_bytes, 24 * 1024 * 1024);
    assert_eq!(suggested.max_response_bytes, 32 * 1024 * 1024);
    assert_eq!(suggested.max_duration, Duration::from_secs(240));
    let maximum = CatalogueDiscoveryBudgetV1::preset(2).unwrap();
    assert_eq!(maximum.suggested_larger(), maximum);
    assert!(CatalogueDiscoveryBudgetV1::preset(3).is_err());
}
