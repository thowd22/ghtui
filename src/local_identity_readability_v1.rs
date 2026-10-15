//! Display-only local context projection. Never parse it into a Git target.
use unicode_segmentation::UnicodeSegmentation;

/// Preserve the recognizable prefix and identifying suffix. Callers retain an
/// always-visible route to the genuine prepared full source explanation.
pub fn compact_context(text: &str, width: u16) -> String {
    let text = if text.len() > 32 * 1024 {
        "Context label exceeds display budget; inspect Source identity"
    } else {
        text
    };
    let width = usize::from(width);
    if width == 0 {
        return String::new();
    }
    if crate::editor::display_width(text) <= width {
        return text.into();
    }
    if width == 1 {
        return "…".into();
    }
    let prefix_budget = (width / 4).min(10);
    let suffix_budget = width.saturating_sub(prefix_budget + 1);
    let mut prefix = String::new();
    let mut cells = 0;
    for grapheme in text.graphemes(true) {
        let next = crate::editor::grapheme_width(grapheme);
        if cells + next > prefix_budget {
            break;
        }
        prefix.push_str(grapheme);
        cells += next;
    }
    let mut suffix = Vec::new();
    let mut cells = 0;
    for grapheme in text.graphemes(true).rev() {
        let next = crate::editor::grapheme_width(grapheme);
        if cells + next > suffix_budget {
            break;
        }
        suffix.push(grapheme);
        cells += next;
    }
    prefix.push('…');
    prefix.extend(suffix.into_iter().rev());
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_local_identity_context_keeps_raw_ref_suffix_without_changing_original() {
        let text = format!(
            "Local /{}/repo / [non-UTF8 ref: na\\xff] / abcdef0123",
            "long-root/".repeat(30)
        );
        let original = text.clone();
        for width in [62, 102] {
            let visible = compact_context(&text, width);
            assert!(visible.starts_with("Local ") && visible.contains('…'));
            assert!(visible.ends_with("[non-UTF8 ref: na\\xff] / abcdef0123"));
            assert!(crate::editor::display_width(&visible) <= usize::from(width));
        }
        assert_eq!(text, original);
    }

    #[test]
    fn long_local_identity_context_cell_bounds_keep_wide_and_combining_graphemes() {
        let text = format!(
            "Local /{}/界e\u{301} / main / {}",
            "界e\u{301}/".repeat(100),
            "a".repeat(40)
        );
        for width in 0..160 {
            let visible = compact_context(&text, width);
            assert!(crate::editor::display_width(&visible) <= usize::from(width));
            assert!(!visible.starts_with('\u{301}'));
        }
        assert_eq!(
            compact_context("Local repo / main", 80),
            "Local repo / main"
        );
    }
}
