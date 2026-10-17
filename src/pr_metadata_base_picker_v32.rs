//! SAME bounded presentation/matcher/geometry, distinct genuine Base catalogue.
//! No LoadedPrMetadataChoices/DTO constructor or eligible-source impersonation.
use super::*;
use crate::providers::{
    gh::{BaseCatalogueV32, BaseCoverageStatusV32},
    runner::CancellationToken,
};
use std::time::Instant;
#[derive(Clone)]
struct RowV32 {
    label: String,
    value: Option<PrMetadataValue>,
    disabled: Option<&'static str>,
    detail: Vec<String>,
}
impl PickerRowView for RowV32 {
    fn label(&self) -> &str {
        &self.label
    }
    fn value(&self) -> Option<&PrMetadataValue> {
        self.value.as_ref()
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
struct RowsV32 {
    rows: Vec<RowV32>,
    complete: bool,
    unavailable: Option<&'static str>,
    notice: Option<&'static str>,
}
impl PickerChoicesView for RowsV32 {
    type Row = RowV32;
    fn choices(&self) -> &[RowV32] {
        &self.rows
    }
    fn kind(&self) -> PickerKindV87 {
        PickerKindV87::Metadata(MetadataChoices::Branches)
    }
    fn complete(&self) -> bool {
        self.complete
    }
    fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable
    }
    fn coverage_notice(&self) -> Option<&str> {
        self.notice
    }
}
#[derive(Clone)]
pub(crate) struct BasePickerV32 {
    catalogue: Arc<BaseCatalogueV32>,
    rows: Arc<RowsV32>,
    state: PickerState,
}
impl std::fmt::Debug for BasePickerV32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BasePickerV32(<actual bounded member presentation; no permission>)")
    }
}
impl BasePickerV32 {
    /// Run during original preparation, before final local SQL/repo/UID close.
    pub(crate) fn prepare(
        catalogue: Arc<BaseCatalogueV32>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, String> {
        let live = || {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                Err("Original Base picker preparation ended".to_owned())
            } else {
                Ok(())
            }
        };
        live()?;
        if catalogue.rows().len() > MAX_OPTIONS {
            return Err("Base picker row cap exceeded".into());
        }
        let mut estimated = 256 * 1024usize;
        for row in catalogue.rows() {
            live()?;
            let text = row
                .name()
                .map_or(0, str::len)
                .checked_add(row.tip().map_or(0, str::len))
                .and_then(|n| n.checked_add(row.disabled().map_or(0, str::len)))
                .and_then(|n| n.checked_mul(4))
                .and_then(|n| n.checked_add(1024))
                .ok_or("Base picker display capacity overflow")?;
            estimated = estimated
                .checked_add(text)
                .filter(|n| *n <= MAX_PREPARED_BYTES)
                .ok_or("Base picker prepared display exceeds2MiB before clone")?;
        }
        if estimated
            .checked_mul(2)
            .and_then(|n| n.checked_add(catalogue.retained()))
            .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err("Base picker retained/transient capacity exceeds8MiB".into());
        }
        let rows=catalogue.rows().iter().map(|row| {
            let label=row.name().map_or_else(||"Branch name unavailable".into(),sanitize_terminal);
            let value=match (row.name(),row.tip()) {
                (Some(name),Some(tip))=>Some(PrMetadataValue::BaseBranch{name:name.into(),commit_sha:tip.into()}),_=>None,
            };
            RowV32{label,value,disabled:row.disabled(),detail:vec![
                format!("Observed full tip: {}",row.tip().unwrap_or("unavailable")),
                format!("Protection: {} (descriptive only, not permission)",match row.protected(){Some(true)=>"reported",Some(false)=>"not reported",None=>"unavailable"}),
                "Only this actual repository member may be chosen; fresh Preview/Confirm rechecks its tip and capability.".into(),
            ]}
        }).collect::<Vec<_>>();
        let coverage = catalogue.coverage();
        let prepared = Arc::new(RowsV32 {
            rows,
            complete: coverage.status == BaseCoverageStatusV32::Complete,
            unavailable: if coverage.status == BaseCoverageStatusV32::Unavailable {
                Some(coverage.reason.unwrap_or("Branch endpoint unavailable"))
            } else {
                None
            },
            notice: coverage.reason,
        });
        let parent = catalogue.actual().parent();
        let header = SourceHeader {
            scope: format!(
                "{} #{} • repository #{} / PR #{}",
                parent.repository(),
                parent.number(),
                parent.repository_id(),
                parent.database_id()
            ),
            counts: format!(
                "Pages {} / {} requested • rows {} read / {} excluded / {} duplicate",
                coverage.fetched_pages,
                coverage.requested_pages,
                coverage.fetched_rows,
                coverage.excluded_rows,
                coverage.duplicates
            ),
            accounting: "Same original read closes before this picker is installed".into(),
            captures: "Choose changes only local selection; no request or publication occurs"
                .into(),
            limits: "Hard bounds:10 pages /1000 rows /2MiB prepared display".into(),
            diagnostics: vec![format!(
                "Current PR Base: {} at {} • branch tip may differ",
                parent.summary().base_ref,
                parent.base_tip_sha()
            )],
            invalid: false,
        };
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
    #[cfg(test)]
    pub(crate) fn catalogue(&self) -> &Arc<BaseCatalogueV32> {
        &self.catalogue
    }
    #[cfg(test)]
    pub(crate) fn selected_index(&self) -> Option<usize> {
        self.state.selected_index()
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
