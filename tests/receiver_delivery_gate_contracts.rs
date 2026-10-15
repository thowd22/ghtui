#[path = "support/receiver_delivery_gate_v1.rs"]
mod gate;
use std::time::{Duration, Instant};
#[test]
fn owned_input_frames_v1_delivery_gate_requires_positive_frame_and_bounded_latency() {
    assert!(gate::ReceiverDeliveryGateV1::new(Duration::from_millis(200), 0).is_err());
    assert!(gate::ReceiverDeliveryGateV1::new(Duration::ZERO, 1).is_err());
    assert!(gate::ReceiverDeliveryGateV1::new(Duration::from_millis(201), 1).is_err());
    assert!(gate::ReceiverDeliveryGateV1::new(Duration::from_millis(200), 1).is_ok());
}
#[test]
fn owned_input_frames_v1_delivery_gate_arms_after_write_and_never_renews_deadline() {
    let now = Instant::now();
    let mut hold = gate::ReceiverDeliveryGateV1::new(gate::MAX_HOLD, 90).unwrap();
    assert_eq!(hold.remaining(now), None);
    hold.flushed(b"\t", 90, now).unwrap();
    assert_eq!(hold.remaining(now), Some(gate::MAX_HOLD));
    hold.flushed(b"\t", 90, now + Duration::from_millis(150))
        .unwrap();
    assert_eq!(
        hold.remaining(now + Duration::from_millis(150)),
        Some(Duration::from_millis(50))
    );
    assert_eq!(hold.remaining(now + Duration::from_millis(200)), None);
    assert_eq!(gate::maximum_unacknowledged(hold.writes()), 2);
    assert_eq!(hold.writes()[0].bytes, b"\t");
    assert_eq!(hold.into_writes().len(), 2);
}
#[test]
fn owned_input_frames_v1_delivery_gate_journal_bounds_and_changed_frame_are_exact() {
    let now = Instant::now();
    let mut hold = gate::ReceiverDeliveryGateV1::new(gate::MAX_HOLD, 90).unwrap();
    assert!(hold.flushed(b"", 90, now).is_err());
    assert!(hold.flushed(&[b'x'; 65], 90, now).is_err());
    assert!(hold.flushed(b"\t", 89, now).is_err());
    assert!(hold.writes().is_empty());
    for frame in [90, 90, 90, 91, 92] {
        hold.flushed(b"\t", frame, now).unwrap();
    }
    assert_eq!(gate::maximum_unacknowledged(hold.writes()), 3);
    for _ in 5..128 {
        hold.flushed(b"\t", 93, now).unwrap();
    }
    assert!(hold.flushed(b"\t", 94, now).is_err());
    assert_eq!(hold.writes().len(), 128);
}

#[test]
fn owned_input_frames_v1_literal_gate_preserves_exact_original_bytes_across_all_packet_splits() {
    let now = Instant::now();
    let needle = b"genuine overlay title";
    let bytes = b"prefix genuine overlay title original footer\x1b[?25l";
    for split in 0..=bytes.len() {
        let mut gate = gate::ReceiverDeliveryGateV1::after_literal(needle, 1).unwrap();
        let mut delivered = Vec::new();
        let mut clock = now;
        for part in [&bytes[..split], &bytes[split..]] {
            if gate.remaining(clock).is_some() {
                clock += gate::MAX_HOLD;
                let tail = gate.release_tail(clock).unwrap_or_default();
                delivered.extend(tail);
            }
            let count = gate.receiver_chunk(part, clock).unwrap();
            delivered.extend_from_slice(&part[..count]);
        }
        delivered.extend(
            gate.release_tail(clock + gate::MAX_HOLD)
                .unwrap_or_default(),
        );
        assert_eq!(
            delivered, bytes,
            "No invented/omitted/duplicated bytes at split{split}"
        );
        gate.ensure_triggered().unwrap();
    }
}
#[test]
fn owned_input_frames_v1_literal_gate_only_exact_contiguous_bytes_trigger_and_no_deadline_renewal()
{
    let now = Instant::now();
    let mut gate = gate::ReceiverDeliveryGateV1::after_literal(b"owned title", 1).unwrap();
    assert_eq!(gate.receiver_chunk(b"owned \x1b[0mtitle", now).unwrap(), 15);
    assert_eq!(gate.remaining(now), None);
    assert!(gate.ensure_triggered().is_err());
    assert_eq!(
        gate.receiver_chunk(b"owned title actual footer", now)
            .unwrap(),
        11
    );
    assert_eq!(gate.remaining(now), Some(gate::MAX_HOLD));
    assert!(gate
        .release_tail(now + Duration::from_millis(199))
        .is_none());
    gate.flushed(b"\t", 1, now + Duration::from_millis(150))
        .unwrap();
    assert_eq!(
        gate.remaining(now + Duration::from_millis(150)),
        Some(Duration::from_millis(50))
    );
    assert_eq!(
        gate.release_tail(now + gate::MAX_HOLD).unwrap(),
        b" actual footer"
    );
    gate.ensure_triggered().unwrap();
}
#[test]
fn owned_input_frames_v1_literal_gate_bounds_refuse_without_pattern_or_chunk_growth() {
    assert!(gate::ReceiverDeliveryGateV1::after_literal(b"", 1).is_err());
    assert!(gate::ReceiverDeliveryGateV1::after_literal(&[b'x'; 129], 1).is_err());
    let mut hold = gate::ReceiverDeliveryGateV1::after_literal(b"title", 1).unwrap();
    assert!(hold.receiver_chunk(&[0; 8193], Instant::now()).is_err());
    assert!(hold.ensure_triggered().is_err());
}
