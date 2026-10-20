//! TASK-99.2: ref chip order, synced pairs, explicit overflow and the label summary.
use super::*;

fn chip(kind: Kind, raw: &str, target: &str) -> HistoryRefV1 {
    let name = ["refs/heads/", "refs/remotes/", "refs/tags/"]
        .iter()
        .find_map(|prefix| raw.strip_prefix(prefix))
        .unwrap_or(raw);
    HistoryRefV1 {
        kind,
        name: name.into(),
        raw_name: raw.as_bytes().to_vec(),
        target: target.into(),
        current: false,
        upstream: None,
    }
}
fn labels(chips: &[RefChip]) -> Vec<&str> {
    chips.iter().map(|chip| chip.label.as_str()).collect()
}

#[test]
fn history_refs_v992_attached_head_leads_then_current_branch_locals_remotes_tags() {
    let mut main = chip(Kind::Local, "refs/heads/main", "a");
    main.current = true;
    let refs = [
        chip(Kind::Tag, "refs/tags/v1.0.0", "a"),
        chip(Kind::Remote, "refs/remotes/upstream/main", "a"),
        chip(Kind::Local, "refs/heads/alpha", "a"),
        main,
    ];
    let shown = chips(&refs);
    assert_eq!(
        labels(&shown),
        ["HEAD", "main", "alpha", "upstream/main", "v1.0.0"]
    );
    assert_eq!(shown[0].kind, Kind::Head);
    assert!(shown[1].current && !shown[2].current);
    assert_eq!(
        shown.iter().map(|chip| chip.kind).collect::<Vec<_>>(),
        [
            Kind::Head,
            Kind::Local,
            Kind::Local,
            Kind::Remote,
            Kind::Tag
        ]
    );
}

#[test]
fn history_refs_v992_detached_head_is_one_typed_chip_not_a_name() {
    let refs = [
        chip(Kind::Head, "HEAD", "a"),
        chip(Kind::Tag, "refs/tags/v0.1.0", "a"),
    ];
    let shown = chips(&refs);
    assert_eq!(labels(&shown), ["HEAD", "v0.1.0"]);
    assert!(shown.iter().all(|chip| !chip.label.contains("graph load")));
}

#[test]
fn history_refs_v992_local_and_its_remote_on_one_commit_merge_into_a_synced_chip() {
    // Same name on origin without an upstream: one chip with the marker.
    let shown = chips(&[
        chip(Kind::Local, "refs/heads/release/1.0", "a"),
        chip(Kind::Remote, "refs/remotes/origin/release/1.0", "a"),
        chip(Kind::Tag, "refs/tags/v1.0.0", "a"),
    ]);
    assert_eq!(labels(&shown), ["release/1.0 ⇅", "v1.0.0"]);
    assert!(shown[0].synced && shown[0].kind == Kind::Local);

    // A configured upstream with another name pairs; the same-named origin branch then stays.
    let mut work = chip(Kind::Local, "refs/heads/work", "a");
    work.upstream = Some(b"refs/remotes/fork/remote-work".to_vec());
    let shown = chips(&[
        work,
        chip(Kind::Remote, "refs/remotes/fork/remote-work", "a"),
        chip(Kind::Remote, "refs/remotes/origin/work", "a"),
    ]);
    assert_eq!(labels(&shown), ["work ⇅", "origin/work"]);

    // Another remote's same-named branch, or one on another commit, is not a pair.
    let shown = chips(&[
        chip(Kind::Local, "refs/heads/main", "a"),
        chip(Kind::Remote, "refs/remotes/mirror/main", "a"),
    ]);
    assert_eq!(labels(&shown), ["main", "mirror/main"]);
    let shown = chips(&[chip(Kind::Local, "refs/heads/main", "a")]);
    assert_eq!(labels(&shown), ["main"]);
    // An upstream that is another local branch never consumes a remote chip.
    let mut local = chip(Kind::Local, "refs/heads/topic", "a");
    local.upstream = Some(b"refs/heads/main".to_vec());
    let shown = chips(&[local, chip(Kind::Remote, "refs/remotes/origin/topic", "a")]);
    assert_eq!(labels(&shown), ["topic", "origin/topic"]);
}

#[test]
fn history_refs_v992_fit_shows_every_chip_that_fits_and_counts_the_rest() {
    let many = (0..6)
        .map(|index| chip(Kind::Tag, &format!("refs/tags/t{index}"), "a"))
        .collect::<Vec<_>>();
    let shown = chips(&many);
    // Each ` tN ` chip is 4 cells, one blank between: all six need 29 cells.
    assert_eq!(fit(&shown, 29), (6, 0));
    assert_eq!(fit(&shown, 100), (6, 0));
    // 28 cells: four chips (19) + blank + ` +2 ` (4) = 24 fits, five would need 29.
    assert_eq!(fit(&shown, 28), (4, 2));
    assert_eq!(fit(&shown, 14), (2, 4));
    // The first chip always shows, even when nothing else fits.
    assert_eq!(fit(&shown, 0), (1, 5));
    assert_eq!(fit(&[], 0), (0, 0));
    assert_eq!(overflow_label(5), "+5");
}

#[test]
fn history_refs_v992_long_names_are_cut_in_rows_not_in_details() {
    let long = "x".repeat(60);
    let shown = chips(&[chip(Kind::Local, &format!("refs/heads/{long}"), "a")]);
    assert_eq!(shown[0].label, long, "the chip keeps the whole name");
    assert_eq!(shown[0].width(), MAX_CHIP_CELLS + 2);
}

#[test]
fn history_refs_v992_summary_lists_every_name_then_an_explicit_overflow() {
    let refs = (0..300)
        .map(|index| chip(Kind::Tag, &format!("refs/tags/v{index:03}"), "a"))
        .collect::<Vec<_>>();
    let shown = chips(&refs);
    let all = summary(&shown[..3], 1024);
    assert_eq!(
        all, "tag v000, tag v001, tag v002",
        "no three-name truncation"
    );
    let capped = summary(&shown, 1024);
    assert!(capped.len() <= 1024 + 8, "{}", capped.len());
    let (names, more) = capped.rsplit_once(", +").unwrap();
    let listed = names.split(", ").count();
    assert_eq!(listed + more.parse::<usize>().unwrap(), 300);
    assert!(listed > 3);
}
