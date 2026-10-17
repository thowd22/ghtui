//! Test-only request acknowledgement from actual completed terminal cells.
//! An old retained result is never completion of a newly submitted search.
pub struct SearchCompletionV1 {
    after: usize,
    query: String,
    source: String,
    result: String,
    loading: Option<usize>,
    ready: Option<usize>,
}
impl SearchCompletionV1 {
    pub fn new(after: usize, query: &str, source: &str, result: &str) -> Self {
        assert!(after > 0);
        for value in [query, source, result] {
            assert!(!value.is_empty() && value.len() <= 512 && !value.contains(['\n', '\r']));
        }
        Self {
            after,
            query: query.into(),
            source: source.into(),
            result: result.into(),
            loading: None,
            ready: None,
        }
    }
    pub fn observe(&mut self, frame: &vt100::Screen, end: usize) {
        if end <= self.after || self.ready.is_some() {
            return;
        }
        let text = frame.contents();
        let query = format!("[/ Query] {}", self.query);
        if !text.contains("Search • focused view")
            || !text.contains(&self.source)
            || !text
                .lines()
                .any(|line| line.trim_matches([' ', '│']) == query)
            || !super::main_footer_visible(frame)
        {
            return;
        }
        if text.contains("Loading Search") && text.contains("[Esc Cancel]") {
            self.loading.get_or_insert(end);
        } else if self.loading.is_some_and(|loading| end > loading)
            && text.contains("Search complete • exact source and exclusions shown")
            && text.contains(&self.result)
            && !text.contains("[Esc Cancel]")
        {
            self.ready = Some(end);
        }
    }
    pub fn completed(&self) -> Option<(usize, usize)> {
        self.loading.zip(self.ready)
    }
}
