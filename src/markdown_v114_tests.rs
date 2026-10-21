use super::*;

/// The worklappy-style body: a Devin review badge in HTML comments, a linked `<picture>`,
/// rules and `##` headings.
pub(crate) const BADGE_BODY: &str = "<!-- devin-review-badge-begin -->\n<a href=\"https://app.devin.ai/review/acme/infra/pull/42\" target=\"_blank\">\n  <picture>\n    <source media=\"(prefers-color-scheme: dark)\" srcset=\"https://static.devin.ai/assets/gh-open-in-devin-review-dark.svg?v=1\">\n    <img src=\"https://static.devin.ai/assets/gh-open-in-devin-review-light.svg?v=1\" alt=\"Open with Devin\">\n  </picture>\n</a>\n<!-- devin-review-badge-end -->\n\n---\n\n## Summary\n\nMoves the **ingest** workers to `europe-west2` and adds a [runbook](https://example.invalid/runbook).\n\n## Test plan\n\n- [x] `terraform plan` is clean\n- [ ] Apply in staging\n  - watch the *ingest* dashboard\n\n---\n\n> Note: rollback is `git revert`.\n\n```hcl\nregion = \"europe-west2\"\n```\n\n| Env | Region |\n| --- | ---: |\n| prod | europe-west2 |\n";

fn kinds(body: &str) -> Vec<BlockKind> {
    render(body).iter().map(|line| line.kind).collect()
}

#[test]
fn markdown_v114_badge_body_hides_comments_and_reduces_html_to_readable_text() {
    let lines = plain_lines(BADGE_BODY);
    let all = lines.join("\n");
    for raw in [
        "<!--",
        "-->",
        "devin-review-badge",
        "<a ",
        "href=",
        "<picture",
        "<source",
        "<img",
        "target=",
        "</a>",
        "srcset",
        "**",
        "```",
        "## ",
    ] {
        assert!(!all.contains(raw), "raw markup {raw:?} leaked:\n{all}");
    }
    assert_eq!(lines[0], "[image: Open with Devin][1]");
    assert!(lines.contains(&"Summary".to_string()), "{lines:#?}");
    assert!(lines.contains(&"Test plan".to_string()));
    assert!(lines
        .iter()
        .any(|line| line == "Moves the ingest workers to europe-west2 and adds a runbook[2]."));
    assert!(lines.contains(&"• ☑ terraform plan is clean".to_string()));
    assert!(lines.contains(&"• ☐ Apply in staging".to_string()));
    assert!(lines.contains(&"  ◦ watch the ingest dashboard".to_string()));
    assert!(lines.contains(&"│ Note: rollback is git revert.".to_string()));
    assert!(lines.contains(&"┃ hcl".to_string()));
    assert!(lines.contains(&"┃ region = \"europe-west2\"".to_string()));
    assert!(
        lines.contains(&"Env  │       Region".to_string()),
        "{lines:#?}"
    );
    assert!(lines.contains(&"prod │ europe-west2".to_string()));
    // Links keep their text inline; the URLs are listed once under the body.
    let links = lines.iter().position(|line| line == "Links").unwrap();
    assert_eq!(
        lines[links + 1],
        "[1] https://app.devin.ai/review/acme/infra/pull/42"
    );
    assert_eq!(lines[links + 2], "[2] https://example.invalid/runbook");
    let kinds = kinds(BADGE_BODY);
    assert!(kinds.contains(&BlockKind::Heading(2)));
    assert!(kinds.contains(&BlockKind::Rule));
    assert!(kinds.contains(&BlockKind::Code));
    assert!(kinds.contains(&BlockKind::TableHeader));
    assert!(kinds.contains(&BlockKind::TableRule));
    assert!(kinds.contains(&BlockKind::TableRow));
    assert_ne!(
        kinds[0],
        BlockKind::Blank,
        "hidden comments leave no leading gap"
    );
}

#[test]
fn markdown_v114_inline_styles_carry_the_right_emphasis() {
    let mut links = Vec::new();
    let spans = inline(
        "a **bold** *it* ~~old~~ `x*y` [t](https://u.invalid) snake_case_name 2 * 3",
        Inline::default(),
        &mut links,
    );
    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(text, "a bold it old x*y t[1] snake_case_name 2 * 3");
    let style = |needle: &str| spans.iter().find(|s| s.text == needle).unwrap().style;
    assert!(style("bold").bold);
    assert!(style("it").italic);
    assert!(style("old").strike);
    assert!(style("x*y").code);
    assert!(style("t").link);
    assert!(style("[1]").marker);
    assert_eq!(links, vec!["https://u.invalid".to_string()]);
}

#[test]
fn markdown_v114_entities_escapes_autolinks_and_bare_urls() {
    assert_eq!(
        plain_lines("Fish &amp; chips &lt;b&gt; \\*literal\\* &#x2713;"),
        vec!["Fish & chips <b> *literal* ✓"]
    );
    let lines = plain_lines("see <https://a.invalid/x> and https://b.invalid/y.");
    assert_eq!(
        lines,
        vec!["see https://a.invalid/x and https://b.invalid/y."]
    );
    // A link whose text is its URL is not listed again.
    assert_eq!(
        plain_lines("[https://c.invalid](https://c.invalid)"),
        vec!["https://c.invalid"]
    );
    // Inline code keeps HTML-looking text verbatim.
    assert_eq!(plain_lines("use `<div>` here"), vec!["use <div> here"]);
}

#[test]
fn markdown_v114_details_summary_br_and_unclosed_comment() {
    let body = "<details>\n<summary>Logs</summary>\n\nline one<br>line two\n</details>\n<!-- trailing never closed\nhidden";
    let lines = plain_lines(body);
    assert_eq!(lines, vec!["▸ Logs", "", "line one", "line two"]);
}

#[test]
fn markdown_v114_setext_headings_ordered_lists_and_nested_quotes() {
    let body = "Title\n=====\n\nSub\n---\n\n1. first\n2) second\n\n> outer\n> > inner";
    let lines = render(body);
    assert_eq!(lines[0].kind, BlockKind::Heading(1));
    assert_eq!(lines[0].plain(), "Title");
    assert_eq!(lines[2].kind, BlockKind::Heading(2));
    let plain = lines.iter().map(MdLine::plain).collect::<Vec<_>>();
    assert!(plain.contains(&"1. first".to_string()));
    assert!(plain.contains(&"2. second".to_string()));
    assert!(plain.contains(&"│ outer".to_string()));
    assert!(plain.contains(&"│ │ inner".to_string()), "{plain:#?}");
    let inner = lines
        .iter()
        .find(|line| line.plain() == "│ │ inner")
        .unwrap();
    assert_eq!(inner.quote, 2);
}

#[test]
fn markdown_v114_wrap_hangs_continuations_and_fits_80_and_140_columns() {
    let body = format!(
        "- {}\n\n> {}\n\n{}\n\n---",
        "word ".repeat(40),
        "quoted ".repeat(30),
        "plain ".repeat(50)
    );
    for width in [78usize, 138] {
        for line in render(&body) {
            let rows = line.wrap(width);
            assert!(!rows.is_empty());
            for (index, row) in rows.iter().enumerate() {
                let text: String = row.iter().map(|s| s.text.as_str()).collect();
                assert!(
                    crate::editor::display_width(text.trim_end()) <= width,
                    "{width}: {text:?}"
                );
                if index > 0 && line.kind == BlockKind::ListItem {
                    assert!(text.starts_with("  "), "list continuation hangs: {text:?}");
                }
                if index > 0 && line.quote > 0 {
                    assert!(text.starts_with("│ "), "quote gutter repeats: {text:?}");
                }
            }
            if line.kind == BlockKind::Rule {
                let text: String = rows[0].iter().map(|s| s.text.as_str()).collect();
                assert_eq!(crate::editor::display_width(&text), width);
            }
        }
    }
    // Wrapping never loses or reorders content.
    let line = &render(&format!("**{}** tail", "bold ".repeat(30)))[0];
    let joined: String = line
        .wrap(20)
        .iter()
        .flatten()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(joined, line.plain());
}

#[test]
fn markdown_v114_real_shaped_corpus_never_shows_tags_or_control_bytes() {
    let corpus = [
        BADGE_BODY,
        "## What\r\nFixes #12\r\n\r\n## Why\r\nBecause.\r\n",
        "<p align=\"center\"><img width=\"200\" src=\"x.png\"></p>\n\n<h2>Changes</h2>\n<ul><li>one</li><li>two</li></ul>",
        "<!-- Please describe your change -->\n\n### Checklist\n- [ ] Tests\n- [ ] Docs",
        "Bumps [lodash](https://github.com/lodash/lodash) from 4.17.20 to 4.17.21.\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a href=\"https://github.com/lodash/lodash/commit/f299b52\"><code>f299b52</code></a> Bump to v4.17.21</li>\n</ul>\n</details>\n<br />\n\n[![Dependabot compatibility score](https://dependabot-badges.githubapp.com/badges/compatibility_score?x=1)](https://docs.github.com/en/github)",
        "```\nunterminated fence <b>kept</b>\n",
        "Escape \u{1b}[31mred\u{1b}[0m bytes",
        "",
    ];
    for body in corpus {
        for line in plain_lines(body) {
            for raw in [
                "<!--", "<a ", "<img", "<p ", "<h2>", "<ul>", "<li>", "<details", "<summary", "<br",
            ] {
                assert!(!line.contains(raw), "{raw} in {line:?} of {body:?}");
            }
            assert!(!line.chars().any(|c| c.is_control()), "{line:?}");
        }
    }
    let dependabot = plain_lines(corpus[4]).join("\n");
    assert!(dependabot.contains("▸ Commits"));
    assert!(
        dependabot.contains("• f299b52[2] Bump to v4.17.21"),
        "{dependabot}"
    );
    assert!(dependabot.contains("[image: Dependabot compatibility score][3]"));
    let html = plain_lines(corpus[2]);
    assert!(html.contains(&"[image]".to_string()), "{html:#?}");
    assert!(html.contains(&"Changes".to_string()));
    assert!(html.contains(&"• one".to_string()));
    // Code inside an unterminated fence stays verbatim.
    assert!(plain_lines(corpus[5]).contains(&"┃ unterminated fence <b>kept</b>".to_string()));
    assert!(plain_lines("").is_empty());
}

#[test]
fn markdown_v114_styles_use_theme_roles() {
    let theme = Theme::from_settings(0, false);
    let lines = render("# Head\n\nbody [l](https://x.invalid) `c`");
    let head = &lines[0];
    assert_eq!(
        style(theme, head, head.spans[0].style),
        theme.role(Role::Accent).add_modifier(Modifier::BOLD)
    );
    let body = &lines[2];
    let link = body.spans.iter().find(|s| s.style.link).unwrap();
    assert_eq!(
        style(theme, body, link.style),
        theme.role(Role::Info).add_modifier(Modifier::UNDERLINED)
    );
    let code = body.spans.iter().find(|s| s.style.code).unwrap();
    assert_eq!(style(theme, body, code.style), theme.role(Role::Info));
    let rendered = super::lines(theme, "x", 10);
    assert_eq!(rendered.len(), 1);
}
