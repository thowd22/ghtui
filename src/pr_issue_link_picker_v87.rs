//! SAME bounded picker geometry/input, but actual Issue catalogue and closed
//! presentation kind. No fake Label/Branch value or eligible DTO constructor.
use super::*;
use crate::providers::{gh::IssueLinkCatalogueV87, runner::CancellationToken};
use std::time::Instant;
struct IssueRowV87 {
    label: String,
    disabled: Option<&'static str>,
    detail: Vec<String>,
}
impl PickerRowView for IssueRowV87 {
    fn label(&self) -> &str {
        &self.label
    }
    fn value(&self) -> Option<&PrMetadataValue> {
        None
    }
    fn actor(&self) -> Option<&crate::identity::ActorIdentity> {
        None
    }
    fn disabled_reason(&self) -> Option<&str> {
        self.disabled
    }
    fn role(&self) -> SemanticToken {
        SemanticToken::Reference
    }
    fn extra_detail(&self) -> &[String] {
        &self.detail
    }
}
struct IssueRowsV87 {
    rows: Vec<IssueRowV87>,
    complete: bool,
    supported: bool,
}
impl PickerChoicesView for IssueRowsV87 {
    type Row = IssueRowV87;
    fn choices(&self) -> &[Self::Row] {
        &self.rows
    }
    fn kind(&self) -> PickerKindV87 {
        PickerKindV87::IssueLinks
    }
    fn complete(&self) -> bool {
        self.complete
    }
    fn unavailable_reason(&self) -> Option<&str> {
        (!self.supported).then_some("Native host lacks the exact manual-link mutation/read contract; no unsupported write will be attempted")
    }
    fn coverage_notice(&self) -> Option<&str> {
        (!self.complete).then_some(
            "Only these actual observed Issue pages are known; unseen membership remains unknown",
        )
    }
}
#[derive(Clone)]
pub(crate) struct IssueLinkPickerV87 {
    catalogue: Arc<IssueLinkCatalogueV87>,
    rows: Arc<IssueRowsV87>,
    state: PickerState,
}
impl std::fmt::Debug for IssueLinkPickerV87 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IssueLinkPickerV87(<actual bounded catalogue; navigation not permission>)")
    }
}
impl IssueLinkPickerV87 {
    pub(crate) fn prepare(
        catalogue: Arc<IssueLinkCatalogueV87>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, String> {
        let live = || {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                Err("Original Issue picker preparation ended".to_owned())
            } else {
                Ok(())
            }
        };
        live()?;
        if catalogue.len() > 200 {
            return Err("Actual Issue catalogue row bound exceeded".into());
        }
        let mut estimated = 256 * 1024usize;
        for index in 0..catalogue.len() {
            let row = catalogue
                .row_v87(index)
                .ok_or("Actual Issue row unavailable")?;
            let strings = [row.repository, row.node, row.url, row.title, row.state]
                .into_iter()
                .try_fold(0usize, |n, s| {
                    n.checked_add(s.len()).ok_or("Issue display overflow")
                })?;
            estimated = estimated
                .checked_add(
                    strings
                        .checked_mul(12)
                        .and_then(|n| n.checked_add(4096 + 2 * std::mem::size_of::<IssueRowV87>()))
                        .ok_or("Issue row overflow")?,
                )
                .filter(|n| *n <= MAX_PREPARED_BYTES)
                .ok_or("Issue picker exceeds2MiB before any row copy")?;
        }
        estimated
            .checked_mul(2)
            .and_then(|n| n.checked_add(catalogue.retained_bound_v87()))
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or("Issue catalogue/picker simultaneous prepared bound exceeded")?;
        let mut rows = Vec::with_capacity(catalogue.len());
        for index in 0..catalogue.len() {
            live()?;
            let row = catalogue
                .row_v87(index)
                .ok_or("Actual Issue row unavailable")?;
            rows.push(IssueRowV87{label:format!("{}#{} • {}",row.repository,row.number,sanitize_terminal(row.title)),disabled:(!row.enabled).then_some("Not an actual eligible manual-link choice for this command"),
                detail:vec![format!("Issue source: {}",row.url),format!("Exact Issue node: {} / numeric {}",row.node,row.id),
                format!("Actual repository: {} / numeric {}",row.repository,row.repository_id),
                format!("Observed Issue state: {}",row.state),format!("PR references [all, manual, nonmanual]: {:?}",row.selected_observations),
                "Choose changes local selection only. Preview re-reads complete sets on BOTH PR and Issue.".into(),
                "Manual development links are not closing keywords, body edits or Projects items.".into()]});
        }
        let prepared = Arc::new(IssueRowsV87 {
            rows,
            complete: catalogue.complete(),
            supported: catalogue.supported(),
        });
        let parent = catalogue.parent();
        let header=SourceHeader{scope:format!("{}#{} • PR #{} / repository #{}",parent.repository(),parent.number(),parent.database_id(),parent.repository_id()),
            counts:format!("{} actual observed Issues • membership {}",catalogue.len(),if catalogue.complete(){"complete"}else{"partial"}),
            accounting:"Same original Core closes before picker installation".into(),captures:"Choose is local; no hidden request or publication".into(),
            limits:"1–4 bounded pages /200 Issues /2MiB prepared display".into(),diagnostics:vec!["Both selected objects need freshly verified capability before one explicit Confirm".into()],invalid:false};
        let state = PickerState::prepare(prepared.as_ref(), header);
        if state.blocked {
            return Err(state.message);
        }
        live()?;
        Ok(Self {
            catalogue,
            rows: prepared,
            state,
        })
    }
    pub(crate) fn key(
        &self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<PrMetadataPickerAction> {
        self.state.key(code, modifiers)
    }
    pub(crate) fn dispatch(&mut self, action: PrMetadataPickerAction) -> PrMetadataPickerOutcome {
        let catalogue = &self.catalogue;
        self.state.dispatch(self.rows.as_ref(), action, |index| {
            catalogue.choose(index).map(|_| ()).map_err(|e| e.message)
        })
    }
    pub(crate) fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: Theme,
    ) -> Vec<PrMetadataPickerHit> {
        self.state.draw(frame, self.rows.as_ref(), area, theme)
    }
    pub(crate) fn mouse(
        &self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        hits: &[PrMetadataPickerHit],
    ) -> Option<PrMetadataPickerAction> {
        self.state.mouse(kind, column, row, hits)
    }
}
