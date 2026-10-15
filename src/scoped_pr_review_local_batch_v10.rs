//! Bounded LOCAL editor map. An index/field is never a fresh canonical range,
//! server comment ID, approval or a GitHub delete request.
use super::*;

#[derive(Clone, Debug)]
struct EntryV10 {
    original: usize,
    key: Arc<str>,
}

#[derive(Clone, Debug)]
pub(super) struct LocalBatchV10 {
    entries: Arc<[EntryV10]>,
    kept: u32,
    painted: Option<(usize, u64)>,
}
impl LocalBatchV10 {
    pub(super) fn prepare(
        values: &ReviewDraftValuesV2,
        enabled: bool,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<(Self, Vec<Field>), String> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original local review editor preparation ended".into());
        }
        values.validate()?; // <=32 /8files /128KiB before body/field copies
        let comments = if enabled { values.comments() } else { &[] };
        let mut fields = Vec::with_capacity(comments.len());
        let mut entries = Vec::with_capacity(comments.len());
        for (index, comment) in comments.iter().enumerate() {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return Err("Original local review editor preparation ended".into());
            }
            let key: Arc<str> = format!("local-review-comment-{index}").into();
            fields.push(Field::text(
                &key,
                &format!("Saved local comment #{}", index + 1),
                comment.body(),
                true,
            )?);
            entries.push(EntryV10 {
                original: index,
                key,
            });
        }
        let kept = if entries.len() == 32 {
            u32::MAX
        } else {
            (1u32 << entries.len()) - 1
        };
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original local review editor preparation ended before result".into());
        }
        Ok((
            Self {
                entries: entries.into(),
                kept,
                painted: None,
            },
            fields,
        ))
    }
    pub(super) fn count(&self) -> usize {
        self.kept.count_ones() as usize
    }
    pub(super) fn mask(&self) -> u32 {
        self.kept
    }
    pub(super) fn clear_paint(&mut self) {
        self.painted = None;
    }
    fn kept_entries(&self) -> impl Iterator<Item = &EntryV10> {
        self.entries
            .iter()
            .filter(|entry| self.kept & (1u32 << entry.original) != 0)
    }
    pub(super) fn validate_fields(&self, fields: &[Field]) -> Result<(), String> {
        if fields.len() != self.count() {
            return Err(
                "Local review field count changed independently of its retained map".into(),
            );
        }
        for (entry, field) in self.kept_entries().zip(fields) {
            if field.key != entry.key.as_ref()
                || field.secret
                || !field.multiline
                || field.editor.text.len() > 65_536
            {
                return Err(
                    "Local review field no longer matches its original bounded comment".into(),
                );
            }
        }
        Ok(())
    }
    pub(super) fn comments(
        &self,
        fields: &[Field],
        original: &ReviewDraftValuesV2,
    ) -> Result<Vec<ReviewDraftCommentV2>, String> {
        self.validate_fields(fields)?;
        self.kept_entries()
            .zip(fields)
            .map(|(entry, field)| {
                let old = original
                    .comments()
                    .get(entry.original)
                    .ok_or("Original local review comment absent")?;
                Ok(ReviewDraftCommentV2 {
                    anchor: old.anchor.clone(), // SAME original metadata allocation, never retargeted
                    body: if old.body.as_ref() == field.editor.text {
                        old.body.clone()
                    } else {
                        Arc::from(field.editor.text.as_str())
                    },
                })
            })
            .collect()
    }
    pub(super) fn preflight(
        &self,
        fields: &[Field],
        original: &ReviewDraftValuesV2,
        summary: &str,
        additional: Option<(&ReviewAnchorMetadataV2, &str)>,
    ) -> Result<(), String> {
        self.validate_fields(fields)?;
        if self
            .kept_entries()
            .any(|entry| original.comments().get(entry.original).is_none())
        {
            return Err("Original local review comment absent".into());
        }
        ReviewDraftValuesV2::validate_borrowed_v10(
            summary,
            self.kept_entries()
                .zip(fields)
                .map(|(entry, field)| {
                    (
                        original.comments()[entry.original].anchor.as_ref(),
                        field.editor.text.as_str(),
                    )
                })
                .chain(additional),
        )
    }
    pub(super) fn matches_comments(
        &self,
        fields: &[Field],
        original: &ReviewDraftValuesV2,
        requested: &[ReviewDraftCommentV2],
    ) -> bool {
        self.validate_fields(fields).is_ok()
            && requested.len() >= self.count()
            && self
                .kept_entries()
                .zip(fields)
                .zip(requested)
                .all(|((entry, field), current)| {
                    original.comments().get(entry.original).is_some_and(|old| {
                        Arc::ptr_eq(&old.anchor, &current.anchor)
                            && field.editor.text == current.body.as_ref()
                    })
                })
    }
    fn original_for_field(&self, field: usize) -> Option<usize> {
        self.kept_entries()
            .nth(field.checked_sub(1)?)
            .map(|entry| entry.original)
    }
}

/// Tiny current selection only; geometry checks below RESTRICT local edits.
pub fn removal_candidate(form: &Form) -> bool {
    let FormPurpose::PrReviewV2(state) = &form.purpose else {
        return false;
    };
    !form.preview
        && !form.submitting
        && form.publication.is_none()
        && state.action == ReviewRequestedActionV2::CreatePending
        && state.input_visible
        && state.revision.is_some()
        && state
            .local_v10
            .original_for_field(form.active_field)
            .is_some()
        && (form.focus == form.active_field || form.focus >= form.fields.len())
}

/// Called ONLY after current field and the COMPLETE action were painted. It
/// records no source/authority and is cleared by editing, resize and removal.
pub fn mark_removal_painted(form: &mut Form) -> bool {
    let visible = removal_candidate(form)
        && form
            .field_rects
            .get(form.active_field)
            .is_some_and(|rect| !rect.is_empty());
    let FormPurpose::PrReviewV2(state) = &mut form.purpose else {
        return false;
    };
    state.local_v10.painted = if visible {
        state
            .local_v10
            .original_for_field(form.active_field)
            .zip(state.revision)
    } else {
        None
    };
    visible
}

pub fn remove_selected(form: &mut Form) -> Result<(), String> {
    if !removal_candidate(form) {
        return Err("Select a visible saved LOCAL comment; no server comment was deleted".into());
    }
    fields(form)?; // exact all-field map BEFORE any effect
    let FormPurpose::PrReviewV2(state) = &mut form.purpose else {
        unreachable!()
    };
    let original = state
        .local_v10
        .original_for_field(form.active_field)
        .ok_or("Focused field is not a retained local comment")?;
    if state.local_v10.painted != state.revision.map(|revision| (original, revision)) {
        return Err("Inspect the current saved LOCAL comment before removing it".into());
    }
    state.local_v10.kept &= !(1u32 << original);
    state.ticket = Arc::new(PresentationTicketV2); // retire EVERY older request/Preview
    form.fields.remove(form.active_field);
    form.focus = form.active_field.min(form.fields.len().saturating_sub(1));
    form.active_field = form.focus;
    form.field_top = form.focus;
    form.field_rects.clear();
    form.invalidate_plan(); // SAME input epoch + Doc/Preview invalidation
    let FormPurpose::PrReviewV2(state) = &mut form.purpose else {
        unreachable!()
    };
    state.input_visible = false; // require genuine new useful paint
    form.dirty = true;
    form.error = "Removed selected LOCAL draft comment only • nothing sent to GitHub".into();
    Ok(())
}
