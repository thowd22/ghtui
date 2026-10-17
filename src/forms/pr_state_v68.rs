//! Genuine bodyless State constructor. No dummy command or global Form path.
use super::*;
impl Form {
    pub(crate) fn pr_state_v68(
        state: crate::scoped_pr_collaboration_ui_v17::state_v68::StateComposerStateV68,
    ) -> Self {
        let title = state.intent.title.clone();
        Self::state_form_v74(
            title,
            crate::scoped_pr_collaboration_ui_v17::ConversationComposerStateV17::from_state_v68(
                state,
            ),
        )
    }
    pub(crate) fn pr_state_history_v74(
        state: crate::scoped_pr_collaboration_ui_v17::state_v68::history_v74::StateHistoryStateV74,
    ) -> Self {
        let mut form = Self::state_form_v74(state.view.title.clone(), crate::scoped_pr_collaboration_ui_v17::ConversationComposerStateV17::from_state_history_v74(state));
        form.preview = true;
        form.dirty = false;
        form
    }
    fn state_form_v74(
        title: String,
        state: crate::scoped_pr_collaboration_ui_v17::ConversationComposerStateV17,
    ) -> Self {
        Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title,
            purpose: FormPurpose::PrCollaborationV17(Box::new(state)),
            fields: vec![],
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: true,
            submit_requested: false,
            editor_requested: false,
            error: String::new(),
            submitting: false,
            field_rects: vec![],
            submitted: false,
            preview_requested: false,
            preview_scroll: 0,
            preview_display_v1: Default::default(),
            error_scroll: 0,
            error_rect: Rect::default(),
            field_top: 0,
            picker: None,
            picker_requested: None,
            publication: None,
            actions_publication: None,
            actions_original_lines: vec![],
            restored_draft: false,
            metadata_preview: None,
            metadata_picker: None,
            metadata_picker_hits: vec![],
            base_selection: None,
            metadata_original_lines: vec![],
        }
    }
}
