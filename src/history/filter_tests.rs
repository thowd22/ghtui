//! TASK-99.3: the pure History filter pieces: query parsing into chips, local text matching
//! and hide-mode lane joins through hidden commits.
use super::*;

fn actor(name: &str, email: &str) -> GitActor {
    GitActor {
        name: name.into(),
        email: email.into(),
        timestamp: 0,
    }
}

#[test]
fn history_filter_v993_query_turns_prefixed_words_into_chips() {
    let spec =
        HistoryFilterSpecV993::default().with_query("author:ada  crash path:src/ fix branch:main");
    assert_eq!(spec.text, "crash fix");
    assert_eq!(spec.author.as_deref(), Some("ada"));
    assert_eq!(spec.path, Some(RawPath(b"src/".to_vec())));
    assert_eq!(spec.scope, HistoryScopeV993::Branch("main".into()));
    assert_eq!(
        spec.order,
        [
            HistoryChipV993::Author,
            HistoryChipV993::Path,
            HistoryChipV993::Scope
        ]
    );
    assert!(spec.matching() && !spec.is_empty());
    // A chip of the same kind replaces the old one and moves last; the text is replaced.
    let again = spec.with_query("author:grace");
    assert_eq!(again.author.as_deref(), Some("grace"));
    assert_eq!(again.text, "");
    assert_eq!(
        again.order,
        [
            HistoryChipV993::Path,
            HistoryChipV993::Scope,
            HistoryChipV993::Author
        ]
    );
    // Backspace removes the last chip, then the one before it.
    let mut removed = again.clone();
    removed.remove(removed.last_chip().unwrap());
    assert_eq!(removed.author, None);
    assert_eq!(removed.last_chip(), Some(HistoryChipV993::Scope));
    removed.remove(HistoryChipV993::Scope);
    assert_eq!(removed.scope, HistoryScopeV993::All);
    removed.remove(HistoryChipV993::Path);
    assert!(removed.is_empty() && removed.order.is_empty());
    // Scope words, an empty prefix and an oversized query.
    assert_eq!(
        HistoryFilterSpecV993::default()
            .with_query("branch:current")
            .scope,
        HistoryScopeV993::Current
    );
    assert_eq!(spec.with_query("branch:all").scope, HistoryScopeV993::All);
    assert_eq!(
        HistoryFilterSpecV993::default()
            .with_query("author: path:")
            .text,
        "author: path:"
    );
    let long = HistoryFilterSpecV993::default().with_query(&"é".repeat(2000));
    assert!(long.text.len() <= MAX_QUERY && long.text.chars().all(|c| c == 'é'));
    // Hide and scope alone do not flag matches; scope alone still filters the walk.
    let mut scoped = HistoryFilterSpecV993::default();
    scoped.set_scope(HistoryScopeV993::Current);
    assert!(!scoped.matching() && !scoped.is_empty());
}

#[test]
fn history_filter_v993_text_matches_message_author_and_sha_prefix() {
    let ada = actor("Ada Lovelace", "ada@example.invalid");
    let id = "a4a6e459449ecd7022fbfc2db0bd21b8b6202cbd";
    let message = "Parse the config header\n\nHandles the CRLF case.";
    // Case-insensitive fixed strings over the whole message, like `git log -i -F --grep`.
    assert!(text_matches("crlf", message, &ada, id));
    assert!(text_matches("config header", message, &ada, id));
    assert!(!text_matches("config.header", message, &ada, id));
    // Author name or email.
    assert!(text_matches("lovelace", message, &ada, id));
    assert!(text_matches("@example.invalid", message, &ada, id));
    // SHA prefixes need four hex digits and match only at the start.
    assert!(text_matches("a4a6", message, &ada, id));
    let other = actor("Grace Hopper", "grace@example.invalid");
    assert!(!text_matches("a4a", "", &other, id));
    assert!(!text_matches("9449", "", &ada, id));
    // Non-ASCII folding.
    let kat = actor("Kateřina Novák", "kat@example.invalid");
    assert!(text_matches("nová", "", &kat, id));
    assert!(text_matches(
        "ÉTÉ".to_lowercase().as_str(),
        "Été notes",
        &kat,
        id
    ));
}

#[test]
fn history_filter_v993_hidden_parents_join_lanes_through_hidden_commits() {
    // a ─ b(merge of c, d) ─ c ─ e ;  d ─ e ; e ─ f (not loaded)
    let rows = vec![
        ("a", vec!["b"]),
        ("b", vec!["c", "d"]),
        ("c", vec!["e"]),
        ("d", vec!["e"]),
        ("e", vec!["f"]),
    ];
    // b and d hidden: a reaches c and, through d, e; c keeps e; e keeps the unloaded f.
    let parents = hidden_parents(&rows, &[true, false, true, false, true]).unwrap();
    assert_eq!(parents[0], ["c", "e"]);
    assert_eq!(parents[2], ["e"]);
    assert_eq!(parents[4], ["f"]);
    // Everything hidden below a: its lane runs to the unloaded parent.
    let parents = hidden_parents(&rows, &[true, false, false, false, false]).unwrap();
    assert_eq!(parents[0], ["f"]);
    // Nothing hidden: parents are unchanged.
    let parents = hidden_parents(&rows, &[true; 5]).unwrap();
    for (row, parents) in rows.iter().zip(&parents) {
        assert_eq!(&row.1, parents);
    }
    // A shown commit never gains more than 64 parents through hidden ones.
    let mut wide = vec![("tip", vec!["hub"])];
    let names: Vec<String> = (0..70).map(|n| format!("p{n}")).collect();
    wide.push(("hub", names.iter().map(String::as_str).collect()));
    for name in &names {
        wide.push((name.as_str(), Vec::new()));
    }
    let mut visible = vec![true; wide.len()];
    visible[1] = false;
    let error = hidden_parents(&wide, &visible).unwrap_err();
    assert!(error.contains("h shows them dimmed"), "{error}");
}
