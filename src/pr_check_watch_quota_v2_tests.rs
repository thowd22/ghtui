//! Pure scheduling limits only. No constructed access/read observation.
use super::*;
#[test]
fn watch_quota_nonoverlap_and_exact_twelve_settled_ticks() {
    let quota = PrCheckWatchQuotaV2::new();
    for tick in 1..=WATCH_TICKS_V2 {
        let claim = quota.claim().unwrap();
        assert!(!quota.due());
        let (count, bytes) = claim.finish(10).unwrap();
        assert_eq!(count, tick);
        assert_eq!(bytes, tick * 10);
    }
    assert!(quota.claim().is_err());
    assert!(!quota.active());
}
#[test]
fn watch_quota_failed_child_group_keeps_full_reserved_charge_and_stops() {
    let quota = PrCheckWatchQuotaV2::new();
    drop(quota.claim().unwrap());
    let q = quota.quota.lock().unwrap();
    assert_eq!(q.charged, WATCH_GROUP_BYTES_V2);
    assert!(!q.in_flight);
    assert!(!quota.active());
}
#[test]
fn watch_quota_original120s_deadline_and_explicit_stop_refuse_next_tick() {
    let quota = Arc::new(PrCheckWatchQuotaV2 {
        started: Instant::now() - WATCH_DURATION_V2,
        active: AtomicBool::new(true),
        quota: Mutex::new(Quota {
            ticks: 0,
            charged: 0,
            in_flight: false,
            next: Instant::now(),
        }),
    });
    assert!(!quota.due());
    assert!(quota.claim().is_err());
    let fresh = PrCheckWatchQuotaV2::new();
    let claim = fresh.claim().unwrap();
    fresh.stop();
    assert!(claim.finish(0).is_err());
    assert!(!fresh.active());
}
#[test]
fn watch_quota_original_allcharge_32mib_and_overflow_before_next_request() {
    for charged in [WATCH_BYTES_V2, usize::MAX] {
        let quota = PrCheckWatchQuotaV2::new();
        quota.quota.lock().unwrap().charged = charged;
        assert!(!quota.due());
        assert!(quota.claim().is_err());
        assert_eq!(quota.quota.lock().unwrap().ticks, 0);
    }
    let quota = PrCheckWatchQuotaV2::new();
    assert!(quota
        .claim()
        .unwrap()
        .finish(WATCH_GROUP_BYTES_V2 + 1)
        .is_err());
    assert!(!quota.active());
}
