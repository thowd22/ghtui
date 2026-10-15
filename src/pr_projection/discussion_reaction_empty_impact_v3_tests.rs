//! Actual SQLite markers/retained indexed bytes. Reactions do NOT mutate text
//! or persisted context.resolved; no fake Inline/Review fence/ordinal claimed.
use super::*;
#[test]
fn review_threads_v3_reaction_confirmed_empty_impact_keeps_all_bound_bodies_ordinals_and_foreign_scope(
) {
    for (command, kind) in [
        ("thread-reaction-add-v3", PrEffectKind::Reaction),
        ("thread-reaction-remove-v3", PrEffectKind::ReactionRemoved),
    ] {
        let (_dir, database) = fixture();
        let inline = thread_target(42, DiscussionKind::InlineComment);
        let review = thread_target(42, DiscussionKind::Review);
        let foreign = thread_target(43, DiscussionKind::InlineComment);
        let (cache, inline_generation) = cached(&database, &inline);
        let (_, review_generation) = cached(&database, &review);
        let (_, foreign_generation) = cached(&database, &foreign);
        let before = raw_records(&database);
        let write = PrWriteIntent {
            command: command.into(),
            selected_resource: format!("thread-reaction-v3/{}", "a".repeat(64)),
            observed_head: "a".repeat(40),
            values: BTreeMap::from([("content".into(), "heart".into())]),
        };
        let reference = record_confirmed(
            &database,
            &inline,
            &write,
            &PrConfirmedEffect {
                kind,
                resource_id: "7001".into(),
                node_id: Some("ACTUAL_REACTION_NODE_7001".into()),
            },
        );
        assert_eq!(ordinal(&database), None);
        for _ in 0..2 {
            let guard =
                guard_discussion_reads(&database, &inline.scope, &[inline.clone(), review.clone()])
                    .unwrap();
            assert!(guard.recovered_sources().is_empty());
            assert_eq!(visible(&cache, &inline), Some(inline_generation));
            assert_eq!(visible(&cache, &review), Some(review_generation));
            assert_eq!(visible(&cache, &foreign), Some(foreign_generation));
            assert_eq!(ordinal(&database), None);
            assert_eq!(raw_records(&database), before);
        }
        let target = PrWriteTarget::for_source(
            &inline.scope,
            &inline.repository,
            &inline.parent_node,
            inline.number,
        )
        .unwrap();
        let marker = PrWriteJournal::open(&database)
            .unwrap()
            .confirmed_publication(&target, &reference)
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::pr_projection::thread_reaction_impact_v3::categories(&marker, true).unwrap(),
            Some(vec![])
        );
        for command in ["thread-reaction-future-v4", "thread-reaction-add-v2"] {
            let mut poison = marker.clone();
            poison.command = command.into();
            assert!(
                crate::pr_projection::thread_reaction_impact_v3::categories(&poison, true).is_err()
            );
        }
        let mut malformed = marker;
        malformed.selected_resource = "thread-reaction-v3/INVALID".into();
        assert!(
            crate::pr_projection::thread_reaction_impact_v3::categories(&malformed, true).is_err()
        );
    }
}
