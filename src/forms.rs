//! Shared native input dialogs. Editing, previewing and closing never publish.
mod pr_state_v68;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::ui_v1 as inline_ui;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::avatars_v7 as review_avatars;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::operations_v3::ui_v3 as review_operation_ui_v3;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::ui_v2 as review_ui;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::comments_v2::publication::ui as comment_ui_v2;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::comments_v2::reactions_v3::form as reaction_ui_v3;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::file_comments_v75::form as file_comment_ui_v75;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::publication_v1::ui_v1 as thread_ui;
use crate::{
    app::{safe_text, Action, App, HitRegion},
    editor::{grapheme_width, TextBuffer},
    providers::gh::{HostIdentity, PrSummary},
    theme::SemanticToken,
};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrFormKind {
    Comment,
    Approve,
    RequestChanges,
    ReviewComment,
    Title,
    Description,
    Base,
}
impl PrFormKind {
    pub const ALL: [(Self, &'static str); 7] = [
        (Self::Comment, "Add conversation comment"),
        (Self::Approve, "Approve review"),
        (Self::RequestChanges, "Request changes"),
        (Self::ReviewComment, "Submit comment review"),
        (Self::Title, "Edit PR title"),
        (Self::Description, "Edit PR description"),
        (Self::Base, "Change base branch"),
    ];
    pub fn label(self) -> &'static str {
        Self::ALL.iter().find(|(kind, _)| *kind == self).unwrap().1
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Approve => "approve",
            Self::RequestChanges => "request-changes",
            Self::ReviewComment => "review-comment",
            Self::Title => "title",
            Self::Description => "description",
            Self::Base => "base",
        }
    }
}
#[derive(Clone, Debug)]
pub struct PrIntent {
    pub kind: PrFormKind,
    pub target: PrSummary,
    pub identity: HostIdentity,
}
impl PrFormKind {
    pub fn is_review(self) -> bool {
        matches!(
            self,
            Self::Approve | Self::RequestChanges | Self::ReviewComment
        )
    }
    /// Loaded-state refusal for review decisions; publication re-checks.
    pub fn review_disabled_reason(self, target: &PrSummary, viewer: &str) -> Option<String> {
        if !self.is_review() {
            return None;
        }
        if matches!(self, Self::Approve | Self::RequestChanges)
            && !viewer.is_empty()
            && target.pr.author.login.eq_ignore_ascii_case(viewer)
        {
            return Some("You cannot approve or request changes on your own PR.".into());
        }
        if target.merged || target.pr.state != "open" {
            return Some("Reviews can only be submitted on an open PR.".into());
        }
        None
    }
}
impl PrIntent {
    /// One plain sentence naming the exact review effect, shown in the form.
    pub fn review_effect(&self) -> Option<String> {
        let what = match self.kind {
            PrFormKind::Approve => "Approve",
            PrFormKind::RequestChanges => "Request changes on",
            PrFormKind::ReviewComment => "Comment on",
            _ => return None,
        };
        let author = &self.target.pr.author.login;
        Some(format!(
            "{what} {}#{}{} at head {} as @{} • summary {}",
            self.target.pr.repository,
            self.target.pr.number,
            if author.is_empty() {
                String::new()
            } else {
                format!(" by @{author}")
            },
            self.target
                .head_sha
                .get(..7)
                .unwrap_or(&self.target.head_sha),
            self.identity.login,
            if self.kind == PrFormKind::Approve {
                "optional"
            } else {
                "required"
            }
        ))
    }
}
#[derive(Clone, Debug)]
pub struct LocalIntent {
    pub original_stash_source_v1: Option<std::sync::Arc<crate::git::OriginalLocalActorOriginV1>>,
    pub failed_stash_v1: Option<std::sync::Arc<crate::git::FailedStashIntentV1>>,
    pub command: crate::local_actions::LocalCommand,
    pub context: crate::local_actions::LocalContext,
    pub plan: Option<crate::local_actions::LocalPlan>,
    pub selections: BTreeMap<String, crate::local_picker::LocalValue>,
    /// GitHub preview for `LocalCommand::CreatePullRequest` (no Git plan).
    pub pr_create_v1: Option<crate::pr_create_v1::PrCreatePlanV1>,
}
#[derive(Clone, Debug)]
pub struct ActionsIntent {
    pub command: crate::actions_workspace::ActionsCommand,
    pub workspace: crate::actions_workspace::ActionsWorkspace,
    pub resource: String,
    pub plan: Option<crate::actions_workspace::ActionsPlan>,
}
#[derive(Clone, Debug)]
pub struct CompareFormIntent {
    pub intent: crate::comparison::CompareIntent,
    pub plan: Option<crate::comparison::ComparisonPlan>,
}
#[derive(Clone, Debug)]
pub struct PrActionIntent {
    pub command: crate::pr_actions::PrCommand,
    pub context: crate::pr_actions::PrActionContext,
    pub plan: Option<crate::pr_actions::PrPlan>,
}
#[derive(Clone, Debug)]
pub enum FormPurpose {
    PrCollaborationV17(Box<crate::scoped_pr_collaboration_ui_v17::ConversationComposerStateV17>),
    ScopedActionsV3(Box<crate::scoped_actions_publication_ui_v3::ScopedActionsComposerStateV3>),
    ScopedActionsWorkflowV6(Box<crate::scoped_actions_workflow_ui_v6::WorkflowComposerStateV6>),
    ScopedActionsDispatchV7(Box<crate::scoped_actions_dispatch_ui_v7::DispatchComposerStateV7>),
    ScopedWorkflowEditV9(
        Box<crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditComposerStateV9>,
    ),
    PrThread(Box<thread_ui::ThreadComposerStateV1>),
    PrThreadCommentV2(Box<comment_ui_v2::CommentComposerStateV2>),
    PrThreadReactionV3(Box<reaction_ui_v3::ReactionComposerStateV3>),
    PrFileCommentV75(Box<file_comment_ui_v75::FileCommentComposerV75>),
    PrInline(Box<inline_ui::ScopedInlineComposerStateV1>),
    PrReviewV2(Box<review_ui::ReviewComposerStateV2>),
    PrReviewOperationV3(Box<review_operation_ui_v3::ReviewOperationComposerStateV3>),
    Pr(Box<PrIntent>),
    PrMetadata(Box<crate::pr_metadata_native::PrMetadataIntent>),
    PrAction(Box<PrActionIntent>),
    Local(Box<LocalIntent>),
    Actions(Box<ActionsIntent>),
    Compare(Box<CompareFormIntent>),
    Corpus(Box<crate::corpus_ui::CorpusIntent>),
}
#[derive(Clone)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub editor: TextBuffer,
    pub multiline: bool,
    pub secret: bool,
    /// Debug-only privacy. Unlike `secret`, this never masks native editing or
    /// changes input/validation/persistence/permission semantics.
    pub(crate) debug_redacted: bool,
    pub choices: Vec<String>,
    /// Display labels never replace the stable values submitted by indexed actions.
    pub choice_labels: Vec<String>,
}
impl std::fmt::Debug for Field {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Field")
            .field("key", &self.key)
            .field("label", &self.label)
            .field(
                "text",
                &if self.secret || self.debug_redacted {
                    "[REDACTED]"
                } else {
                    self.editor.text.as_str()
                },
            )
            .field("secret", &self.secret)
            .finish()
    }
}
impl Field {
    pub fn text(key: &str, label: &str, value: &str, multiline: bool) -> Result<Self, String> {
        Ok(Self {
            key: key.into(),
            label: label.into(),
            editor: TextBuffer::new(value)?,
            multiline,
            secret: false,
            debug_redacted: false,
            choices: vec![],
            choice_labels: vec![],
        })
    }
    fn choice_label(&self, index: usize) -> &str {
        self.choice_labels
            .get(index)
            .unwrap_or(&self.choices[index])
    }
}
#[derive(Clone, Debug)]
pub struct Form {
    // Logical presentation only. Never source, permission, token or approval.
    presentation_v4: std::sync::Arc<()>,
    presentation_input_epoch_v4: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub title: String,
    pub purpose: FormPurpose,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub active_field: usize,
    pub preview: bool,
    pub dirty: bool,
    pub submit_requested: bool,
    pub editor_requested: bool,
    pub error: String,
    pub submitting: bool,
    pub field_rects: Vec<Rect>,
    pub submitted: bool,
    pub preview_requested: bool,
    pub preview_scroll: usize,
    pub(crate) preview_display_v1: crate::prepared_form_text_v1::FormPreviewDisplayStateV1,
    pub error_scroll: usize,
    pub error_rect: Rect,
    pub field_top: usize,
    pub picker: Option<crate::local_picker::LocalPicker>,
    pub picker_requested: Option<usize>,
    pub publication: Option<crate::pr_write_journal::PrPublicationReference>,
    pub actions_publication: Option<crate::actions_write_journal::ActionsPublicationReference>,
    /// Worker-prepared original metadata; never current edited request values.
    pub actions_original_lines: Vec<String>,
    pub restored_draft: bool,
    /// In-memory exact-effect snapshot; never serialized as draft approval.
    pub metadata_preview: Option<crate::pr_metadata_preview::PrMetadataPreview>,
    pub metadata_picker: Option<crate::pr_metadata_picker::PrMetadataPicker>,
    pub metadata_picker_hits: Vec<crate::pr_metadata_picker::PrMetadataPickerHit>,
    pub base_selection: Option<crate::pr_metadata_base::PrBaseSelection>,
    pub metadata_original_lines: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormAction {
    RefreshConversationV19,
    OpenDispatchRunV7,
    InspectConversationV17,
    AcknowledgeActionsV3,
    AcknowledgeActionsAcceptanceV5,
    RemoveLocalReviewCommentV10,
    InlineAnchor,
    InlineAnchorScroll(i32),
    InspectStashV1,
    Field(usize),
    Preview,
    Editor,
    Submit,
    Close,
    Choice(usize, usize),
    ErrorScroll(i32),
    FieldScroll(i32),
    OpenPicker(usize),
    FullInput(usize),
    Picker(crate::local_picker::PickerAction),
    MetadataPicker(crate::pr_metadata_picker::PrMetadataPickerAction),
    Swap,
    NewIntent,
}
impl Form {
    pub(crate) fn presentation_v4(&self) -> (&std::sync::Arc<()>, Option<u64>) {
        let value = self
            .presentation_input_epoch_v4
            .load(std::sync::atomic::Ordering::Acquire);
        (&self.presentation_v4, (value != u64::MAX).then_some(value))
    }
    pub(crate) fn observe_presentation_input_v4(&mut self) {
        let _ = self.presentation_input_epoch_v4.fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |value| value.checked_add(1),
        );
    }
    pub fn pr_metadata(
        intent: crate::pr_metadata_native::PrMetadataIntent,
    ) -> Result<Self, String> {
        // Use shared native form behavior; retain only the genuine original Arc
        // and a prepared human field, never an editable JSON mutation payload.
        let mut form = Self::pr(PrIntent {
            kind: PrFormKind::Comment,
            target: intent.original.as_ref().clone(),
            identity: intent.identity.clone(),
        })?;
        form.title = format!(
            "{} • {}#{}",
            intent.command.label(),
            intent.original.pr.repository,
            intent.original.pr.number
        );
        form.fields = vec![Field::text("body", "Selected value", "", false)?];
        form.purpose = FormPurpose::PrMetadata(Box::new(intent));
        Ok(form)
    }
    pub fn corpus(intent: crate::corpus_ui::CorpusIntent) -> Result<Self, String> {
        intent.scope()?;
        let fields = crate::corpus_ui::fields(&intent)?;
        Ok(Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title: "Manage selected repositories".into(),
            purpose: FormPurpose::Corpus(Box::new(intent)),
            fields,
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
        })
    }
    pub fn pr(intent: PrIntent) -> Result<Self, String> {
        crate::storage::Scope::github(&intent.identity.host, intent.identity.account_id)
            .map_err(|error| error.to_string())?;
        let (label, value, multiline) = match intent.kind {
            PrFormKind::Title => ("Title", intent.target.pr.title.clone(), false),
            PrFormKind::Description => ("Description", intent.target.pr.body.clone(), true),
            PrFormKind::Base => ("Base branch", intent.target.base_ref.clone(), false),
            kind if kind.is_review() => ("Review summary", String::new(), true),
            _ => ("Body", String::new(), true),
        };
        let title = format!(
            "{} • {}#{}",
            intent.kind.label(),
            intent.target.pr.repository,
            intent.target.pr.number
        );
        Ok(Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title,
            purpose: FormPurpose::Pr(Box::new(intent)),
            fields: vec![Field::text("body", label, &value, multiline)?],
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
        })
    }
    pub fn local(
        command: crate::local_actions::LocalCommand,
        context: crate::local_actions::LocalContext,
    ) -> Result<Self, String> {
        if let Some(reason) = command.disabled_reason(&context) {
            return Err(reason);
        }
        let fields = crate::local_actions::fields(command, &context)
            .into_iter()
            .map(|input| {
                let mut field = Field::text(input.key, input.label, &input.value, input.multiline)?;
                field.choices = input.choices.into_iter().map(str::to_owned).collect();
                Ok(field)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title: command.label().into(),
            purpose: FormPurpose::Local(Box::new(LocalIntent {
                original_stash_source_v1: None,
                failed_stash_v1: None,
                command,
                context,
                plan: None,
                selections: BTreeMap::new(),
                pr_create_v1: None,
            })),
            fields,
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
        })
    }
    pub fn actions(
        workspace: crate::actions_workspace::ActionsWorkspace,
        command: crate::actions_workspace::ActionsCommand,
    ) -> Result<Self, String> {
        crate::storage::Scope::github(&workspace.identity.host, workspace.identity.account_id)
            .map_err(|error| error.to_string())?;
        let resource = workspace
            .resource_key(command)
            .map_err(|error| error.to_string())?;
        let inputs = workspace
            .fields(command)
            .map_err(|error| error.to_string())?;
        Self::actions_inputs_v4(workspace, command, resource, inputs)
    }
    fn actions_inputs_v4(
        workspace: crate::actions_workspace::ActionsWorkspace,
        command: crate::actions_workspace::ActionsCommand,
        resource: String,
        inputs: Vec<crate::actions_workspace::ActionsField>,
    ) -> Result<Self, String> {
        let fields = inputs
            .into_iter()
            .map(|input| {
                let mut field =
                    Field::text(&input.key, &input.label, &input.value, input.multiline)?;
                field.secret = input.secret;
                field.choices = input.choices;
                Ok(field)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self::actions_fields_v3(
            workspace, command, resource, fields,
        ))
    }
    /// Presentation seed only; no Plan, eligibility or approval.
    /// Completed protected originals remain inspectable without fake run state.
    pub(crate) fn actions_original_shell_v3(
        workspace: crate::actions_workspace::ActionsWorkspace,
    ) -> Result<Self, String> {
        Self::actions_original_shell_v4(
            workspace,
            crate::actions_workspace::ActionsCommand::Cancel,
            false,
        )
    }
    pub(crate) fn actions_original_shell_v4(
        workspace: crate::actions_workspace::ActionsWorkspace,
        command: crate::actions_workspace::ActionsCommand,
        history: bool,
    ) -> Result<Self, String> {
        crate::storage::Scope::github(&workspace.identity.host, workspace.identity.account_id)
            .map_err(|error| error.to_string())?;
        let resource = workspace
            .resource_key(command)
            .map_err(|error| error.to_string())?;
        let inputs = if history {
            vec![]
        } else {
            crate::actions_workspace::ActionsWorkspace::original_run_fields_v4(command)
                .ok_or("Unsupported closed original run Form")?
        };
        Self::actions_inputs_v4(workspace, command, resource, inputs)
    }
    pub(crate) fn actions_fields_v3(
        workspace: crate::actions_workspace::ActionsWorkspace,
        command: crate::actions_workspace::ActionsCommand,
        resource: String,
        fields: Vec<Field>,
    ) -> Self {
        Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title: command.label().into(),
            purpose: FormPurpose::Actions(Box::new(ActionsIntent {
                command,
                workspace,
                resource,
                plan: None,
            })),
            fields,
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
    /// Worker-only unpublished W9 seed. No invented run/workspace is needed
    /// to use the SAME Form renderer, editor, focus, errors and actions.
    pub(crate) fn workflow_edit_seed_v9(
        state: Box<crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditComposerStateV9>,
        fields: Vec<Field>,
    ) -> Self {
        Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title: "Edit workflow locally • Preview each exact step".into(),
            purpose: FormPurpose::ScopedWorkflowEditV9(state),
            fields,
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
    pub fn compare(intent: crate::comparison::CompareIntent) -> Result<Self, String> {
        let mut mode = Field::text(
            "mode",
            "Show: snapshot = all differences A→B • proposed = only B's changes since it branched from A (like a PR)",
            "snapshot",
            false,
        )?;
        mode.choices = vec!["snapshot".into(), "proposed".into()];
        if intent.mode == crate::comparison::ComparisonMode::ProposedChanges {
            mode.editor = TextBuffer::new("proposed")?;
        }
        let fields = vec![
            Field::text(
                "base",
                "A: from (branch, tag or commit)",
                &intent.base.value,
                false,
            )?,
            Field::text(
                "head",
                "B: to (branch, tag or commit)",
                &intent.head.value,
                false,
            )?,
            mode,
        ];
        Ok(Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title: "Compare two saved versions".into(),
            purpose: FormPurpose::Compare(Box::new(CompareFormIntent { intent, plan: None })),
            fields,
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
        })
    }
    pub fn pr_action(
        command: crate::pr_actions::PrCommand,
        context: crate::pr_actions::PrActionContext,
    ) -> Result<Self, String> {
        if let Some(reason) = command.disabled_reason(&context) {
            return Err(reason);
        }
        let fields = crate::pr_actions::fields(command, &context)
            .into_iter()
            .map(|input| {
                let mut field = Field::text(input.key, input.label, &input.value, input.multiline)?;
                for choice in input.choices {
                    field.choices.push(choice.value);
                    field.choice_labels.push(choice.label);
                }
                Ok(field)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            presentation_v4: std::sync::Arc::new(()),
            presentation_input_epoch_v4: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            title: format!(
                "{} • {}#{}",
                command.label(),
                context.target.pr.repository,
                context.target.pr.number
            ),
            purpose: FormPurpose::PrAction(Box::new(PrActionIntent {
                command,
                context,
                plan: None,
            })),
            fields,
            focus: 0,
            active_field: 0,
            preview: false,
            dirty: false,
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
        })
    }
    pub fn compare_intent(&self) -> Result<crate::comparison::CompareIntent, String> {
        let FormPurpose::Compare(comparison) = &self.purpose else {
            return Err("Not a saved-version comparison".into());
        };
        let mut intent = comparison.intent.clone();
        let values = self.values();
        intent.edit(
            0,
            values
                .get("base")
                .cloned()
                .ok_or("Starting version is missing")?,
        )?;
        intent.edit(
            1,
            values
                .get("head")
                .cloned()
                .ok_or("Target version is missing")?,
        )?;
        intent.mode = match values.get("mode").map(String::as_str) {
            Some("snapshot") => crate::comparison::ComparisonMode::Snapshot,
            Some("proposed") => crate::comparison::ComparisonMode::ProposedChanges,
            _ => return Err("Choose snapshot or proposed changes explicitly".into()),
        };
        Ok(intent)
    }
    pub fn uses_prepared_preview(&self) -> bool {
        match &self.purpose {
            FormPurpose::Pr(intent) => {
                crate::pr_metadata_preview::PrMetadataPreview::supports(intent.kind)
            }
            _ => true,
        }
    }
    pub fn view(&self) -> crate::app::View {
        match self.purpose {
            FormPurpose::PrCollaborationV17(_) => crate::app::View::Inbox,
            FormPurpose::PrThread(_)
            | FormPurpose::PrFileCommentV75(_)
            | FormPurpose::PrThreadCommentV2(_)
            | FormPurpose::PrThreadReactionV3(_)
            | FormPurpose::PrInline(_)
            | FormPurpose::PrReviewV2(_)
            | FormPurpose::PrReviewOperationV3(_) => crate::app::View::Inbox,
            FormPurpose::Pr(_) | FormPurpose::PrAction(_) | FormPurpose::PrMetadata(_) => {
                crate::app::View::Inbox
            }
            FormPurpose::Local(_) | FormPurpose::Compare(_) => crate::app::View::Repository,
            FormPurpose::Actions(_)
            | FormPurpose::ScopedActionsV3(_)
            | FormPurpose::ScopedActionsWorkflowV6(_)
            | FormPurpose::ScopedActionsDispatchV7(_)
            | FormPurpose::ScopedWorkflowEditV9(_) => crate::app::View::Actions,
            FormPurpose::Corpus(_) => crate::app::View::Search,
        }
    }
    pub fn requires_preview(&self) -> bool {
        if self.actions_publication.is_some() || self.metadata_original() {
            return false; // Viewing the original is not a new remote effect.
        }
        match &self.purpose {
            FormPurpose::Pr(_) => self.uses_prepared_preview(),
            FormPurpose::Compare(_) => false,
            _ => true,
        }
    }
    pub fn metadata_original(&self) -> bool {
        self.publication.is_some()
            && (matches!(&self.purpose,FormPurpose::Pr(intent) if intent.kind==PrFormKind::Base)
                || matches!(self.purpose, FormPurpose::PrMetadata(_)))
    }
    /// Prepare an ordinary local save. Only an exact newly accepted Members
    /// Preview may register its initial key without invalidating that Preview.
    pub fn prepare_draft_autosave_v77(&mut self) {
        if !crate::scoped_pr_collaboration_ui_v17::metadata_members_v48::pending_initial_save_v77(
            self,
        ) && !crate::scoped_pr_collaboration_ui_v17::issue_link_v87::pending_initial_save(self)
        {
            self.invalidate_plan();
        }
    }
    /// Queue acceptance is NOT a durable ACK or permission to publish. The
    /// original DraftPublicationFence still waits for the exact writer result.
    pub fn settle_draft_autosave_v77(&mut self, accepted: bool) {
        crate::scoped_pr_collaboration_ui_v17::metadata_members_v48::settle_initial_save_v77(
            self, accepted,
        );
        crate::scoped_pr_collaboration_ui_v17::issue_link_v87::settle_initial_save(self, accepted);
        if accepted {
            self.dirty = false;
        }
    }
    pub fn invalidate_plan(&mut self) {
        self.observe_presentation_input_v4();
        self.preview_display_v1 = Default::default();
        self.metadata_preview = None;
        if matches!(
            self.purpose,
            FormPurpose::Pr(_)
                | FormPurpose::PrCollaborationV17(_)
                | FormPurpose::PrMetadata(_)
                | FormPurpose::PrInline(_)
                | FormPurpose::PrThread(_)
                | FormPurpose::PrReviewV2(_)
                | FormPurpose::PrReviewOperationV3(_)
                | FormPurpose::PrThreadCommentV2(_)
                | FormPurpose::PrThreadReactionV3(_)
                | FormPurpose::ScopedActionsV3(_)
                | FormPurpose::PrFileCommentV75(_)
                | FormPurpose::ScopedActionsWorkflowV6(_)
                | FormPurpose::ScopedActionsDispatchV7(_)
                | FormPurpose::ScopedWorkflowEditV9(_)
        ) && self.uses_prepared_preview()
        {
            self.preview = false;
            self.preview_scroll = 0;
            self.preview_requested = false;
            self.submit_requested = false;
        }
        match &mut self.purpose {
            FormPurpose::PrCollaborationV17(state) => state.invalidate(),
            FormPurpose::ScopedActionsV3(state) => state.invalidate(),
            FormPurpose::ScopedActionsWorkflowV6(state) => state.invalidate(),
            FormPurpose::ScopedActionsDispatchV7(state) => state.invalidate(),
            FormPurpose::ScopedWorkflowEditV9(state) => state.invalidate(),
            FormPurpose::PrThread(state) => state.invalidate(),
            FormPurpose::PrThreadCommentV2(state) => state.invalidate(),
            FormPurpose::PrThreadReactionV3(state) => state.invalidate(),
            FormPurpose::PrFileCommentV75(state) => state.invalidate(),
            FormPurpose::PrInline(state) => state.invalidate(),
            FormPurpose::PrReviewV2(state) => state.invalidate(),
            FormPurpose::PrReviewOperationV3(state) => state.invalidate(),
            FormPurpose::Local(intent) => {
                intent.plan = None;
                intent.pr_create_v1 = None;
            }
            FormPurpose::Actions(intent) => intent.plan = None,
            FormPurpose::Compare(intent) => intent.plan = None,
            FormPurpose::PrAction(intent) => intent.plan = None,
            FormPurpose::PrMetadata(intent) => {
                intent.plan = None;
                intent.preview_lines.clear();
            }
            FormPurpose::Corpus(intent) => intent.plan = None,
            FormPurpose::Pr(_) => {}
        }
        crate::scoped_actions_dispatch_ui_v7::restore_editable_summary_v152(self);
        crate::scoped_actions_workflow_edit_ui_v9::restore_summary(self);
    }
    pub fn plan_ready(&self) -> bool {
        match &self.purpose {
            FormPurpose::PrCollaborationV17(state) => state.ready(),
            FormPurpose::ScopedActionsV3(state) => state.ready(),
            FormPurpose::ScopedActionsWorkflowV6(state) => state.ready(),
            FormPurpose::ScopedActionsDispatchV7(state) => state.ready(),
            FormPurpose::ScopedWorkflowEditV9(state) => state.ready(),
            FormPurpose::PrThread(state) => state.ready(),
            FormPurpose::PrThreadCommentV2(state) => state.ready(),
            FormPurpose::PrThreadReactionV3(state) => state.ready(),
            FormPurpose::PrFileCommentV75(state) => state.ready(),
            FormPurpose::PrInline(state) => state.ready(),
            FormPurpose::PrReviewV2(state) => state.ready(),
            FormPurpose::PrReviewOperationV3(state) => state.ready(),
            FormPurpose::Local(intent) => intent.plan.is_some() || intent.pr_create_v1.is_some(),
            FormPurpose::Actions(intent) => intent.plan.is_some(),
            FormPurpose::Pr(_) => {
                !self.uses_prepared_preview()
                    || self
                        .metadata_preview
                        .as_ref()
                        .is_some_and(|plan| plan.disabled_reason().is_none())
            }
            FormPurpose::PrAction(intent) => intent.plan.is_some(),
            FormPurpose::PrMetadata(intent) => intent.plan.is_some(),
            FormPurpose::Compare(intent) => intent.plan.is_some(),
            FormPurpose::Corpus(intent) => intent.plan.is_some(),
        }
    }
    pub fn plan_lines(&self) -> &[String] {
        if let FormPurpose::ScopedActionsDispatchV7(state) = &self.purpose {
            return state.plan_lines();
        }
        if let FormPurpose::ScopedActionsWorkflowV6(state) = &self.purpose {
            return state.plan_lines();
        }
        if let FormPurpose::ScopedActionsV3(state) = &self.purpose {
            return state.plan_lines();
        }
        if self.metadata_original() {
            return &self.metadata_original_lines;
        }
        if self.actions_publication.is_some() {
            return &self.actions_original_lines;
        }
        match &self.purpose {
            FormPurpose::PrCollaborationV17(state) => state.plan_lines(),
            FormPurpose::ScopedActionsV3(state) => state.plan_lines(),
            FormPurpose::ScopedActionsWorkflowV6(state) => state.plan_lines(),
            FormPurpose::ScopedActionsDispatchV7(state) => state.plan_lines(),
            FormPurpose::ScopedWorkflowEditV9(state) => state.plan_lines(),
            FormPurpose::PrThread(state) => state.plan_lines(),
            FormPurpose::PrThreadCommentV2(state) => state.plan_lines(),
            FormPurpose::PrThreadReactionV3(state) => state.plan_lines(),
            FormPurpose::PrFileCommentV75(state) => state.plan_lines(),
            FormPurpose::PrInline(state) => state.plan_lines(),
            FormPurpose::PrReviewV2(state) => state.plan_lines(),
            FormPurpose::PrReviewOperationV3(state) => state.plan_lines(),
            FormPurpose::Local(intent) => match &intent.pr_create_v1 {
                Some(plan) => plan.preview.as_slice(),
                None => intent
                    .plan
                    .as_ref()
                    .map_or(&[], |plan| plan.preview.as_slice()),
            },
            FormPurpose::Actions(intent) => intent
                .plan
                .as_ref()
                .map_or(&[], |plan| plan.preview.as_slice()),
            FormPurpose::Pr(_) => self
                .metadata_preview
                .as_ref()
                .map_or(&[], |plan| plan.preview()),
            FormPurpose::PrMetadata(intent) => &intent.preview_lines,
            FormPurpose::Corpus(intent) => intent
                .plan
                .as_ref()
                .map_or(&[], |plan| plan.preview.as_slice()),
            FormPurpose::PrAction(intent) => intent
                .plan
                .as_ref()
                .map_or(&[], |plan| plan.preview.as_slice()),
            FormPurpose::Compare(intent) => intent
                .plan
                .as_ref()
                .map_or(&[], |plan| plan.preview.as_slice()),
        }
    }
    /// ORIGINAL worker only. This display is not source/approval authority.
    pub fn prepare_preview_display_v1(
        &mut self,
        cancel: &crate::providers::runner::CancellationToken,
        original_deadline: std::time::Instant,
    ) -> Result<(), crate::prepared_form_text_v1::PreparedFormTextErrorV1> {
        let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
            self.plan_lines(),
            cancel,
            original_deadline,
        )?;
        self.install_prepared_preview_display_v1(display);
        Ok(())
    }
    pub(crate) fn install_prepared_preview_display_v1(
        &mut self,
        display: std::sync::Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    ) {
        self.preview_display_v1.document = Some(display);
        self.preview_display_v1.viewport = Default::default();
        self.preview_display_v1.geometry_rejected = true; // no useful current paint yet
        self.preview_display_v1.failure = None;
    }
    pub fn clear_preview_display_geometry_v1(&mut self) {
        // INPUT-time resize rejection; retain exact desired display anchor and
        // source/Preview. This marker can only restrict, never grant a write.
        self.preview_display_v1.geometry_rejected = true;
        crate::scoped_pr_collaboration_ui_v17::painted_preview(self, false);
        if let FormPurpose::ScopedActionsV3(state) = &mut self.purpose {
            state.resize();
        }
        if let FormPurpose::ScopedActionsWorkflowV6(state) = &mut self.purpose {
            state.resize();
        }
        if let FormPurpose::ScopedActionsDispatchV7(state) = &mut self.purpose {
            state.resize();
        }
        if let FormPurpose::ScopedWorkflowEditV9(state) = &mut self.purpose {
            state.resize();
        }
    }
    /// Worker-only exact-input recheck, before any remote request or journal claim.
    pub fn validate_metadata_preview(&self) -> Result<(), String> {
        if let FormPurpose::Pr(intent) = &self.purpose {
            if crate::pr_metadata_preview::PrMetadataPreview::supports(intent.kind) {
                self.metadata_preview
                    .as_ref()
                    .ok_or("Choose Preview and inspect the exact effect before confirmation.")?
                    .validate_selected(intent, &self.values(), self.base_selection.as_ref())?;
            }
        }
        Ok(())
    }
    fn confirmation_blocked_reason(&self) -> Option<&str> {
        if matches!(self.purpose, FormPurpose::PrFileCommentV75(_)) && self.publication.is_some() {
            return Some("Protected FILE publication is inspect only • no resend");
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_)) && self.publication.is_some()
        {
            return Some("Protected original Conversation reference is inspect only • no resend");
        }
        if self.preview && self.uses_prepared_preview() && self.preview_display_v1.geometry_rejected
        {
            return Some(self.preview_display_v1.failure.map_or(
                "Resize to inspect prepared Preview (at least64 text cells); no confirmation from a hidden/partial display.",
                |error|error.message()));
        }
        if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) && self.publication.is_some()
        {
            return Some("Protected reaction publication is inspect only • no resend");
        }
        if matches!(self.purpose, FormPurpose::PrThreadCommentV2(_)) && self.publication.is_some() {
            return Some("Protected selected-comment publication is inspect only • no resend");
        }
        if matches!(self.purpose, FormPurpose::PrThread(_)) && self.publication.is_some() {
            return Some("Original thread publication is history, never permission to resend");
        }
        if matches!(
            self.purpose,
            FormPurpose::PrReviewV2(_) | FormPurpose::PrReviewOperationV3(_)
        ) && self.publication.is_some()
        {
            return Some("Original review publication is history, never permission to resend");
        }
        if matches!(self.purpose, FormPurpose::PrInline(_)) && self.publication.is_some() {
            return Some("Original inline publication is history, never permission to resend");
        }
        if matches!(
            self.purpose,
            FormPurpose::ScopedActionsV3(_)
                | FormPurpose::ScopedActionsWorkflowV6(_)
                | FormPurpose::ScopedActionsDispatchV7(_)
        ) && self.actions_publication.is_some()
        {
            return None; // SAME Submit now explicitly Views original, never POST.
        }
        if self.requires_preview() && !self.plan_ready() {
            Some(
                self.metadata_preview
                    .as_ref()
                    .and_then(|plan| plan.disabled_reason())
                    .unwrap_or("Choose Preview and inspect the exact effect before confirmation."),
            )
        } else {
            None
        }
    }
    fn footer_actions(&self) -> Vec<FormAction> {
        if let Some(controls) = crate::scoped_actions_workflow_edit_ui_v9::footer(self) {
            let mut actions: Vec<_> = controls.into_iter().map(|(_, action)| action).collect();
            if !self.error.is_empty() {
                actions.extend([FormAction::ErrorScroll(-3), FormAction::ErrorScroll(3)]);
            }
            if self.fields.len() > 1 && !self.preview {
                actions.extend([FormAction::FieldScroll(-1), FormAction::FieldScroll(1)]);
            }
            return actions;
        }
        if let Some(controls) = crate::scoped_pr_collaboration_ui_v17::footer(self) {
            let mut actions: Vec<_> = controls.into_iter().map(|(_, a)| a).collect();
            if !self.error.is_empty() {
                actions.extend([FormAction::ErrorScroll(-3), FormAction::ErrorScroll(3)]);
            }
            return actions;
        }
        if let Some(controls) = crate::scoped_actions_publication_ui_v3::footer(self) {
            let mut actions: Vec<_> = controls.into_iter().map(|(_, action)| action).collect();
            if !self.error.is_empty() {
                actions.extend([FormAction::ErrorScroll(-3), FormAction::ErrorScroll(3)]);
            }
            return actions;
        }
        let mut actions = vec![
            FormAction::Preview,
            FormAction::Editor,
            FormAction::Submit,
            FormAction::Close,
        ];
        if matches!(self.purpose, FormPurpose::PrFileCommentV75(_)) && self.publication.is_some() {
            actions.retain(|a| *a != FormAction::Editor);
        }
        if matches!(&self.purpose, FormPurpose::PrCollaborationV17(state)
            if !state.editable_body())
        {
            actions.retain(|action| *action != FormAction::Editor);
        }
        if review_ui::local_batch_v10::removal_candidate(self) {
            actions.push(FormAction::RemoveLocalReviewCommentV10);
        }
        if matches!(&self.purpose,FormPurpose::PrReviewOperationV3(state)
            if self.publication.is_some()||!state.operation_intent().action().needs_body())
        {
            actions.retain(|action| *action != FormAction::Editor);
        }
        if matches!(&self.purpose, FormPurpose::PrThread(state) if self.publication.is_some() || state.intent().action() != crate::scoped_pr_observed_tabs_v1::resources::threads_v1::publication_v1::ThreadPublicationActionV1::Reply)
        {
            actions.retain(|action| *action != FormAction::Editor);
        }
        if matches!(&self.purpose,FormPurpose::PrThreadCommentV2(state)if self.publication.is_some()||state.intent().action!=crate::pr_write_journal::thread_comment_history_v2::CommentActionV2::Edit)
        {
            actions.retain(|action| *action != FormAction::Editor);
        }
        if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) {
            actions.retain(|action| *action != FormAction::Editor);
        }
        if !self.error.is_empty() {
            actions.extend([FormAction::ErrorScroll(-3), FormAction::ErrorScroll(3)]);
        }
        if self.fields.len() > 1 {
            actions.extend([FormAction::FieldScroll(-1), FormAction::FieldScroll(1)]);
        }
        if matches!(self.purpose, FormPurpose::Compare(_)) {
            actions.push(FormAction::Swap);
        }
        if self.actions_publication.is_some() || self.metadata_original() {
            actions.push(FormAction::NewIntent);
        }
        actions
    }
    pub fn is_local(&self) -> bool {
        matches!(self.purpose, FormPurpose::Local(_))
    }
    pub fn resource(&self) -> String {
        match &self.purpose {
            FormPurpose::PrCollaborationV17(state) => state.draft_key().into(),
            FormPurpose::ScopedActionsV3(state) => state.opened().intent().key().into(),
            FormPurpose::ScopedActionsWorkflowV6(state) => state.opened().intent().key().into(),
            FormPurpose::ScopedActionsDispatchV7(state) => state.opened().intent().key().into(),
            FormPurpose::ScopedWorkflowEditV9(state) => state.intent().key().into(),
            FormPurpose::PrThread(state) => state.draft_key().into(),
            FormPurpose::PrThreadCommentV2(state) => state.draft_key().into(),
            FormPurpose::PrThreadReactionV3(state) => state.draft_key().into(),
            FormPurpose::PrFileCommentV75(state) => state.draft_key().into(),
            FormPurpose::PrInline(state) => state.draft_key().into(),
            FormPurpose::PrReviewV2(state) => state.draft_key().into(),
            FormPurpose::PrReviewOperationV3(state) => state.draft_key().into(),
            FormPurpose::Pr(intent) => format!(
                "{}/pull/{}/composer/{}",
                intent.target.pr.repository,
                intent.target.pr.number,
                intent.kind.key()
            ),
            FormPurpose::PrAction(intent) => intent.context.resource(intent.command),
            FormPurpose::PrMetadata(intent) => format!(
                "{}/pull/{}/composer/{}",
                intent.original.pr.repository,
                intent.original.pr.number,
                intent.command.key()
            ),
            FormPurpose::Local(intent) => {
                use std::os::unix::ffi::OsStrExt;
                let path = intent
                    .context
                    .directory
                    .as_os_str()
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                // A PR draft belongs to its head branch, not the whole checkout.
                let branch = match intent.command {
                    crate::local_actions::LocalCommand::CreatePullRequest => intent
                        .context
                        .status
                        .branch
                        .as_deref()
                        .map(|branch| format!("/{branch}"))
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                format!("local/{path}/operation/{}{branch}", intent.command.key())
            }
            FormPurpose::Actions(intent) => intent.resource.clone(),
            FormPurpose::Corpus(intent) => intent.resource(),
            FormPurpose::Compare(intent) => {
                use std::os::unix::ffi::OsStrExt;
                let path = intent
                    .intent
                    .directory
                    .as_os_str()
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                format!("local/{path}/compare")
            }
        }
    }
    pub fn scope(&self) -> crate::storage::Scope {
        match &self.purpose {
            FormPurpose::PrCollaborationV17(state) => state.original().scope().clone(),
            FormPurpose::ScopedActionsV3(state) => {
                state.opened().intent().source().original().scope().clone()
            }
            FormPurpose::ScopedActionsWorkflowV6(state) => {
                state.opened().intent().source().original().scope().clone()
            }
            FormPurpose::ScopedActionsDispatchV7(state) => {
                state.opened().intent().source().original().scope().clone()
            }
            FormPurpose::ScopedWorkflowEditV9(state) => {
                state.intent().source().original().scope().clone()
            }
            FormPurpose::PrThread(state) => state.intent().scope().clone(),
            FormPurpose::PrThreadCommentV2(state) => state.intent().scope().clone(),
            FormPurpose::PrThreadReactionV3(state) => state.intent().scope().clone(),
            FormPurpose::PrFileCommentV75(state) => state.intent().scope().clone(),
            FormPurpose::PrInline(state) => state.intent().scope().clone(),
            FormPurpose::PrReviewV2(state) => state.intent().scope().clone(),
            FormPurpose::PrReviewOperationV3(state) => state.intent().scope().clone(),
            FormPurpose::Pr(intent) => crate::storage::Scope {
                host: intent.identity.host.clone(),
                login: format!("user-id:{}", intent.identity.account_id.unwrap_or(0)),
            },
            FormPurpose::PrAction(intent) => crate::storage::Scope {
                host: intent.context.identity.host.clone(),
                login: format!(
                    "user-id:{}",
                    intent.context.identity.account_id.unwrap_or(0)
                ),
            },
            FormPurpose::PrMetadata(intent) => crate::storage::Scope {
                host: intent.identity.host.clone(),
                login: format!("user-id:{}", intent.identity.account_id.unwrap_or(0)),
            },
            FormPurpose::Local(_) | FormPurpose::Compare(_) => crate::storage::Scope {
                host: "local".into(),
                login: "workspace".into(),
            },
            FormPurpose::Actions(intent) => crate::storage::Scope {
                host: intent.workspace.identity.host.clone(),
                login: format!(
                    "user-id:{}",
                    intent.workspace.identity.account_id.unwrap_or(0)
                ),
            },
            FormPurpose::Corpus(intent) => crate::storage::Scope {
                host: intent.identity.host.clone(),
                login: format!("user-id:{}", intent.identity.account_id.unwrap_or(0)),
            },
        }
    }
    pub fn values(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .map(|field| (field.key.clone(), field.editor.text.clone()))
            .collect()
    }
    pub fn field_has_picker(&self, index: usize) -> bool {
        match (&self.purpose, self.fields.get(index)) {
            (FormPurpose::Pr(intent), Some(field)) => {
                intent.kind == PrFormKind::Base && field.key == "body" && self.publication.is_none()
            }
            (FormPurpose::PrMetadata(_), Some(field)) => {
                field.key == "body" && self.publication.is_none()
            }
            (FormPurpose::Local(intent), Some(field)) => {
                crate::local_picker::field_supported(intent.command, &field.key)
            }
            (FormPurpose::Compare(_), Some(field)) => matches!(field.key.as_str(), "base" | "head"),
            _ => false,
        }
    }
    /// Cursor movement is not an edit. Exact inventory identities survive focus changes.
    pub fn field_edited(&mut self, key: &str, before: &str) {
        if self
            .fields
            .iter()
            .any(|field| field.key == key && field.editor.text != before)
        {
            self.base_selection = None;
            if let FormPurpose::PrMetadata(intent) = &mut self.purpose {
                intent.clear_selection();
            }
            self.invalidate_plan();
            if let FormPurpose::Local(intent) = &mut self.purpose {
                intent.selections.remove(key);
            }
            if let FormPurpose::Compare(comparison) = &mut self.purpose {
                let index = match key {
                    "base" => Some(0),
                    "head" => Some(1),
                    _ => None,
                };
                if let Some(index) = index {
                    if let Some(field) = self.fields.iter().find(|field| field.key == key) {
                        if let Err(error) = comparison.intent.edit(index, field.editor.text.clone())
                        {
                            self.error = error;
                        }
                    }
                }
            }
            self.dirty = true;
            self.invalidate_plan();
        }
    }
    pub fn choose_local(&mut self, choice: crate::local_picker::LocalChoice) -> Result<(), String> {
        if let Some(reason) = choice.disabled_reason {
            return Err(reason);
        }
        if let FormPurpose::Compare(comparison) = &mut self.purpose {
            let mut intent = comparison.intent.clone();
            let index = match choice.field.as_str() {
                "base" => 0,
                "head" => 1,
                _ => return Err("Comparison choice belongs to another field".into()),
            };
            intent.select(index, choice.value)?;
            let value = if index == 0 {
                &intent.base.value
            } else {
                &intent.head.value
            };
            if value != &choice.editor_text {
                return Err("Comparison choice does not match its exact saved identity".into());
            }
            self.fields[index].editor = TextBuffer::new(value)?;
            comparison.intent = intent;
            comparison.plan = None;
            self.focus = index;
            self.active_field = index;
            self.preview = false;
            self.dirty = true;
            return Ok(());
        }
        let FormPurpose::Local(intent) = &mut self.purpose else {
            return Err("Local inventory does not belong to this form.".into());
        };
        let expected = choice.value.editor_text(intent.command, &choice.field)?;
        if expected != choice.editor_text {
            return Err("Inventory label does not match its exact target.".into());
        }
        let index = self
            .fields
            .iter()
            .position(|field| field.key == choice.field)
            .ok_or("Inventory choice belongs to another field")?;
        self.fields[index].editor = TextBuffer::new(&expected)?;
        intent.selections.insert(choice.field, choice.value);
        intent.plan = None;
        self.focus = index;
        self.active_field = index;
        self.preview = false;
        self.dirty = true;
        Ok(())
    }
    pub fn draft(&self) -> String {
        if matches!(self.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
            return crate::scoped_actions_workflow_edit_ui_v9::encoded_draft(self)
                .unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::ScopedActionsDispatchV7(_)) {
            return serde_json::to_string(&serde_json::json!({"version":7,"purpose":"scoped-actions-dispatch-v7","actions_publication":self.actions_publication})).unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_)) {
            return crate::scoped_pr_collaboration_ui_v17::encoded_draft(self).unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::ScopedActionsWorkflowV6(_)) {
            return serde_json::to_string(&serde_json::json!({"version":6,"purpose":"scoped-actions-workflow-v6","actions_publication":self.actions_publication})).unwrap_or_default();
        }
        if matches!(
            self.purpose,
            FormPurpose::ScopedActionsV3(_)
                | FormPurpose::ScopedActionsWorkflowV6(_)
                | FormPurpose::ScopedActionsDispatchV7(_)
        ) {
            return serde_json::to_string(&serde_json::json!({
                "version":3,"purpose":"scoped-actions-v3","values":self.values(),
                "actions_publication":self.actions_publication,
            }))
            .unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::PrFileCommentV75(_)) {
            return file_comment_ui_v75::encoded_draft(self).unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::PrReviewOperationV3(_)) {
            return review_operation_ui_v3::encoded_draft(self).unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) {
            return reaction_ui_v3::encoded_draft(self).unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::PrThreadCommentV2(_)) {
            return comment_ui_v2::encoded_draft(self).unwrap_or_default();
        }
        if matches!(self.purpose, FormPurpose::PrReviewV2(_)) {
            return review_ui::encoded_draft(self).unwrap_or_default();
        }
        let values: BTreeMap<&str, &str> = self
            .fields
            .iter()
            .filter(|field| !field.secret)
            .map(|field| (field.key.as_str(), field.editor.text.as_str()))
            .collect();
        if matches!(
            self.purpose,
            FormPurpose::Pr(_)
                | FormPurpose::PrAction(_)
                | FormPurpose::PrMetadata(_)
                | FormPurpose::PrInline(_)
                | FormPurpose::PrThread(_)
        ) {
            serde_json::to_string(
                &serde_json::json!({"version":2,"values":values,"publication":self.publication}),
            )
            .unwrap_or_default()
        } else if matches!(self.purpose, FormPurpose::Actions(_)) {
            serde_json::to_string(&serde_json::json!({
                "version":3,"purpose":"actions","values":values,
                "actions_publication":self.actions_publication,
            }))
            .unwrap_or_default()
        } else if let FormPurpose::Local(intent) = &self.purpose {
            serde_json::to_string(&serde_json::json!({
                "version": 1, "values": values, "selections": intent.selections,
            }))
            .unwrap_or_default()
        } else if let FormPurpose::Compare(comparison) = &self.purpose {
            let selections = [
                ("base", comparison.intent.base.selection.as_ref()),
                ("head", comparison.intent.head.selection.as_ref()),
            ]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect::<BTreeMap<_, _>>();
            serde_json::to_string(
                &serde_json::json!({"version":1,"values":values,"selections":selections}),
            )
            .unwrap_or_default()
        } else {
            serde_json::to_string(&values).unwrap_or_default()
        }
    }
    /// Actual W9 form only; callers receive a failure rather than silently
    /// replacing a recoverable draft with an empty encoding.
    pub fn workflow_edit_draft_v9(&self) -> Result<String, String> {
        crate::scoped_actions_workflow_edit_ui_v9::encoded_draft(self)
    }
    /// Only the actual owning DraftWriter ACK calls this presentation helper.
    /// Raw database/trigger diagnostics are not a private YAML/message channel.
    /// No dirty flag, request, source, save retry or approval is changed.
    pub fn workflow_edit_draft_error_v9(&mut self) -> bool {
        if !matches!(self.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
            return false;
        }
        self.error="Local workflow draft save failed; exact editable text is retained. Inspect local storage, then edit explicitly to retry. Nothing was replayed or published.".into();
        true
    }
    pub fn restore_draft(&mut self, body: &str) -> Result<(), String> {
        if matches!(self.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
            return Err("Workflow local-edit text requires its actual bounded fresh opener; generic restoration grants no receipt/source/approval".into());
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_)) {
            return Err("Original Conversation requires its fresh bounded opener; generic restore cannot grant body/approval".into());
        }
        if matches!(
            self.purpose,
            FormPurpose::ScopedActionsV3(_)
                | FormPurpose::ScopedActionsWorkflowV6(_)
                | FormPurpose::ScopedActionsDispatchV7(_)
        ) {
            return Err("Original Actions protected references require fresh source inspection; generic draft restoration grants no approval".into());
        }
        if matches!(self.purpose, FormPurpose::PrFileCommentV75(_)) {
            return Err("FILE drafts require fresh original member/source opener • protected history cannot restore writer approval".into());
        }
        if matches!(self.purpose, FormPurpose::PrReviewOperationV3(_)) {
            return Err("Original pending operation text requires its fresh source opener; protected history cannot become a writer".into());
        }
        if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) {
            return Err("Reaction drafts require a fresh original-source opener; protected history cannot restore writer approval".into());
        }
        self.observe_presentation_input_v4();
        if matches!(self.purpose, FormPurpose::PrThreadCommentV2(_)) {
            return Err("Comment drafts require the fresh original-source opener • protected history cannot be restored into a writer".into());
        }
        if matches!(self.purpose, FormPurpose::PrThread(_)) {
            return Err("Thread drafts require fresh original-source opener; generic restore cannot adopt history or approval".into());
        }
        if matches!(self.purpose, FormPurpose::PrReviewV2(_)) {
            return Err("Review text/anchors require their exact original-source opener; generic restore cannot recreate approval".into());
        }
        if matches!(self.purpose, FormPurpose::PrInline(_)) {
            return Err("Inline drafts require a fresh original-source draft opener; generic restore cannot adopt fields or approval".into());
        }
        let parsed: serde_json::Value =
            serde_json::from_str(body).map_err(|error| error.to_string())?;
        let structured = parsed.get("version").is_some();
        let version = parsed.get("version").and_then(serde_json::Value::as_u64);
        let pr_draft = matches!(
            self.purpose,
            FormPurpose::Pr(_) | FormPurpose::PrAction(_) | FormPurpose::PrMetadata(_)
        );
        let actions_draft = matches!(self.purpose, FormPurpose::Actions(_));
        if structured
            && version != Some(1)
            && !(pr_draft && version == Some(2))
            && !(actions_draft && version == Some(3))
        {
            return Err("Unsupported local draft version; no values were restored.".into());
        }
        if actions_draft
            && version == Some(3)
            && parsed.get("purpose").and_then(serde_json::Value::as_str) != Some("actions")
        {
            return Err("Saved draft purpose is not Actions; no values were restored.".into());
        }
        let actions_publication: Option<crate::actions_write_journal::ActionsPublicationReference> =
            if actions_draft && version == Some(3) {
                serde_json::from_value(
                    parsed
                        .get("actions_publication")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                )
                .map_err(|error| error.to_string())?
            } else {
                None
            };
        if let Some(reference) = &actions_publication {
            if reference.attempt_id <= 0
                || reference.fingerprint_sha256.len() != 64
                || !reference
                    .fingerprint_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(
                    "Saved original Actions reference is invalid; no values were restored.".into(),
                );
            }
        }
        let publication: Option<crate::pr_write_journal::PrPublicationReference> =
            if pr_draft && version == Some(2) {
                serde_json::from_value(
                    parsed
                        .get("publication")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                )
                .map_err(|error| error.to_string())?
            } else {
                None
            };
        if let Some(reference) = &publication {
            let node = match &self.purpose {
                FormPurpose::Pr(intent) => &intent.target.pr.id,
                FormPurpose::PrAction(intent) => &intent.context.target.pr.id,
                FormPurpose::PrMetadata(intent) => &intent.original.pr.id,
                _ => unreachable!(),
            };
            if reference.pr_node_id != *node
                || reference.attempt_id <= 0
                || !matches!(reference.observed_head.len(), 40 | 64)
                || !reference
                    .observed_head
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || reference.payload_sha256.len() != 64
                || !reference
                    .payload_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("Saved publication metadata does not match this exact PR/attempt; no draft fields were restored.".into());
            }
        }
        let values: BTreeMap<String, String> = serde_json::from_value(if structured {
            parsed
                .get("values")
                .cloned()
                .ok_or("Draft values are missing")?
        } else {
            parsed.clone()
        })
        .map_err(|error| error.to_string())?;
        // Validate all buffers before replacing any live value or typed identity.
        let editors = self
            .fields
            .iter()
            .map(|field| {
                if field.secret {
                    Ok(None)
                } else {
                    values
                        .get(&field.key)
                        .map(|value| TextBuffer::new(value).map(Some))
                        .unwrap_or(Ok(None))
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        if let FormPurpose::Local(intent) = &mut self.purpose {
            let selections: BTreeMap<String, crate::local_picker::LocalValue> = if structured {
                serde_json::from_value(
                    parsed
                        .get("selections")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({})),
                )
                .map_err(|error| error.to_string())?
            } else {
                BTreeMap::new()
            };
            for (key, selection) in &selections {
                let expected = selection.editor_text(intent.command, key)?;
                if !self
                    .fields
                    .iter()
                    .any(|field| &field.key == key && !field.secret)
                    || values.get(key) != Some(&expected)
                {
                    return Err(
                        "Draft inventory identity does not match its operation field.".into(),
                    );
                }
            }
            intent.selections = selections;
            intent.plan = None;
        }
        if let FormPurpose::Compare(comparison) = &mut self.purpose {
            let mut intent = comparison.intent.clone();
            intent.base.selection = None;
            intent.head.selection = None;
            for (index, key) in ["base", "head"].into_iter().enumerate() {
                if let Some(value) = values.get(key) {
                    intent.edit(index, value.clone())?;
                }
            }
            let selections: BTreeMap<String, crate::local_picker::LocalValue> = if structured {
                serde_json::from_value(
                    parsed
                        .get("selections")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({})),
                )
                .map_err(|error| error.to_string())?
            } else {
                BTreeMap::new()
            };
            for (key, selection) in selections {
                let index = match key.as_str() {
                    "base" => 0,
                    "head" => 1,
                    _ => {
                        return Err("Comparison draft contains an unrelated inventory field".into());
                    }
                };
                intent.select(index, selection)?;
                let expected = if index == 0 {
                    &intent.base.value
                } else {
                    &intent.head.value
                };
                if values.get(&key) != Some(expected) {
                    return Err(
                        "Comparison draft selection does not match its saved identity".into(),
                    );
                }
            }
            if let Some(mode) = values.get("mode") {
                intent.mode = match mode.as_str() {
                    "snapshot" => crate::comparison::ComparisonMode::Snapshot,
                    "proposed" => crate::comparison::ComparisonMode::ProposedChanges,
                    _ => return Err("Unknown comparison question in saved draft".into()),
                };
            }
            comparison.intent = intent;
            comparison.plan = None;
        }
        for (field, editor) in self.fields.iter_mut().zip(editors) {
            if let Some(editor) = editor {
                field.editor = editor;
            }
        }
        self.publication = publication;
        self.actions_publication = actions_publication;
        self.actions_original_lines.clear();
        self.restored_draft = true;
        if let FormPurpose::PrMetadata(intent) = &mut self.purpose {
            intent.clear_selection();
        }
        self.base_selection = None;
        self.metadata_picker = None;
        self.metadata_picker_hits.clear();
        self.metadata_original_lines.clear();
        self.invalidate_plan();
        Ok(())
    }
    fn full_input_available(&self, index: usize) -> bool {
        !self.preview
            && !self.submitting
            && matches!(&self.purpose,FormPurpose::Local(intent)if intent.command==crate::local_actions::LocalCommand::RemoveWorktree)
            && self
                .fields
                .get(index)
                .is_some_and(|field| field.key == "path" && !field.secret)
    }
    fn retire_changed_current_input(&mut self) -> bool {
        let Some(picker) = self
            .picker
            .as_ref()
            .filter(|picker| picker.is_current_input())
        else {
            return false;
        };
        let valid = match &self.purpose {
            FormPurpose::Local(intent)
                if intent.command == crate::local_actions::LocalCommand::RemoveWorktree =>
            {
                self.fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.key == "path" && !field.secret)
                    .is_some_and(|(index, field)| {
                        self.full_input_available(index)
                            && picker.current_input_matches(
                                &intent.context.directory,
                                &field.key,
                                &field.editor.text,
                            )
                    })
            }
            _ => false,
        };
        if !valid {
            self.picker = None;
            self.error =
                "Full input view retired after its intent changed; nothing chosen or verified."
                    .into();
        }
        !valid
    }
    pub fn dispatch(&mut self, action: &FormAction) {
        if self.submitting && !matches!(action, FormAction::Close) {
            return;
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_))
            && self.publication.is_some()
            && matches!(action, FormAction::Editor | FormAction::Choice(..))
        {
            self.error =
                "Protected original Conversation body/choices are read only • inspect history"
                    .into();
            return;
        }
        if crate::scoped_actions_workflow_edit_ui_v9::dispatch(self, action) {
            return;
        }
        if crate::scoped_actions_publication_ui_v3::dispatch(self, action) {
            return;
        }
        if crate::scoped_pr_collaboration_ui_v17::dispatch(self, action) {
            return;
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_))
            && *action == FormAction::Submit
            && crate::scoped_pr_collaboration_ui_v17::confirmed_preview(self).is_err()
        {
            self.error =
                "Fresh exact usefully painted Conversation Preview required before Confirm".into();
            return;
        }
        if let FormPurpose::PrReviewOperationV3(state) = &self.purpose {
            let return_to_fields_v1 =
                self.preview && *action == FormAction::Preview && self.publication.is_none();
            if matches!(
                action,
                FormAction::Preview | FormAction::Editor | FormAction::Submit
            ) && !return_to_fields_v1
                && (!state.input_visible() || self.publication.is_some())
            {
                self.error="Resize to inspect original pending operation; protected history is inspect only".into();
                return;
            }
            if *action == FormAction::Submit
                && review_operation_ui_v3::confirmed_preview(self).is_err()
            {
                self.error =
                    "Fresh useful exact pending operation Preview required before Confirm".into();
                return;
            }
        }
        self.observe_presentation_input_v4();
        if let FormPurpose::PrReviewV2(state) = &self.purpose {
            let return_to_fields_v1 =
                self.preview && *action == FormAction::Preview && self.publication.is_none();
            if matches!(
                action,
                FormAction::Preview | FormAction::Editor | FormAction::Submit
            ) && !return_to_fields_v1
                && (!state.input_visible() || self.publication.is_some())
            {
                self.error="Resize to inspect current review; original publication history is read-only, never resend approval".into();
                return;
            }
            if *action == FormAction::Submit && review_ui::confirmed_preview(self).is_err() {
                self.error =
                    "Fresh usefully painted exact review Preview required before Confirm".into();
                return;
            }
        }
        match action {
            FormAction::AcknowledgeActionsV3 | FormAction::AcknowledgeActionsAcceptanceV5 => {}
            FormAction::RemoveLocalReviewCommentV10 => {
                if let Err(reason) = review_ui::local_batch_v10::remove_selected(self) {
                    self.error = reason;
                }
            }
            FormAction::InlineAnchor => inline_ui::anchor_action(self, None),
            FormAction::InlineAnchorScroll(delta) => inline_ui::anchor_action(self, Some(*delta)),
            FormAction::InspectStashV1 => {}
            FormAction::FullInput(index) if self.full_input_available(*index) => {
                let FormPurpose::Local(intent) = &self.purpose else {
                    return;
                };
                let field = &self.fields[*index];
                match crate::local_picker::LocalPicker::current_input(
                    &intent.context.directory,
                    &field.key,
                    &field.editor.text,
                ) {
                    Ok(picker) => self.picker = Some(picker),
                    Err(error) => self.error = error,
                }
            }
            FormAction::FullInput(_) => {}
            FormAction::Field(index) => {
                self.focus = (*index).min(self.fields.len().saturating_sub(1));
                self.active_field = self.focus;
            }
            FormAction::Preview if self.uses_prepared_preview() => {
                if self.preview {
                    self.preview = false;
                } else {
                    self.preview_requested = true;
                }
            }
            FormAction::Preview => self.preview = !self.preview,
            FormAction::Editor if self.fields.is_empty() => {
                self.error = "This operation has no text field to edit.".into()
            }
            FormAction::Editor
                if self.publication.is_some()
                    && matches!(self.purpose, FormPurpose::PrThread(_)) =>
            {
                self.error = "Original thread publication is inspect-only; its saved body is not a new draft or resend approval.".into();
            }
            FormAction::Editor if matches!(self.purpose, FormPurpose::PrMetadata(_)) => {
                self.error="Metadata values use genuine indexed choices; external JSON/text cannot grant selection or publication authority. Use F4 Choose.".into();
            }
            FormAction::Editor if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) => {
                self.error="Reactions have no editable body or external editor. Fresh Preview/Confirm selects the exact typed content/owned ID.".into();
            }
            FormAction::Editor if matches!(self.purpose, FormPurpose::PrFileCommentV75(_)) => {
                match file_comment_ui_v75::validate_input(self, "") {
                    Ok(()) => self.editor_requested = true,
                    Err(reason) => self.error = reason,
                }
            }
            FormAction::Editor if matches!(self.purpose, FormPurpose::PrThreadCommentV2(_)) => {
                match comment_ui_v2::validate_input(self, "") {
                    Ok(()) => self.editor_requested = true,
                    Err(reason) => self.error = reason,
                }
            }
            FormAction::Editor
                if self
                    .fields
                    .get(self.active_field)
                    .is_some_and(|field| field.secret) =>
            {
                self.error =
                    "Secret input stays transient; external-editor files are not used for secrets."
                        .into()
            }
            FormAction::Editor => self.editor_requested = true,
            FormAction::Submit
                if !self.submitted
                    || self.actions_publication.is_some()
                    || self.metadata_original() =>
            {
                if let Some(reason) = self.confirmation_blocked_reason() {
                    self.error = reason.into();
                } else {
                    self.submit_requested = true;
                }
            }
            FormAction::Choice(index, choice) => {
                if let Some(field) = self.fields.get_mut(*index) {
                    let key = field.key.clone();
                    let before = field.editor.text.clone();
                    if let Some(value) = field.choices.get(*choice) {
                        field.editor = TextBuffer::new(value).unwrap_or_default();
                        self.dirty = true;
                        self.focus = *index;
                        self.active_field = *index;
                    }
                    self.field_edited(&key, &before);
                }
            }
            FormAction::OpenPicker(index) if self.field_has_picker(*index) => {
                self.invalidate_plan();
                self.base_selection = None;
                if let FormPurpose::PrMetadata(intent) = &mut self.purpose {
                    intent.clear_selection();
                }
                self.picker_requested = Some(*index);
                self.preview = false;
            }
            FormAction::Picker(action) => {
                if self.retire_changed_current_input() {
                    return;
                }
                let outcome = self
                    .picker
                    .as_mut()
                    .map(|picker| picker.dispatch(action.clone()));
                match outcome {
                    Some(crate::local_picker::PickerOutcome::Chosen(choice)) => {
                        if let Err(error) = self.choose_local(*choice) {
                            self.error = error;
                        }
                        self.picker = None;
                    }
                    Some(crate::local_picker::PickerOutcome::Closed) => self.picker = None,
                    _ => {}
                }
            }
            FormAction::MetadataPicker(action) => {
                let outcome = self
                    .metadata_picker
                    .as_mut()
                    .map(|picker| picker.dispatch(action.clone()));
                match outcome {
                    Some(crate::pr_metadata_picker::PrMetadataPickerOutcome::Chosen(index)) => {
                        if let FormPurpose::PrMetadata(intent) = &mut self.purpose {
                            match intent.choose(
                                self.metadata_picker.as_ref().expect("active picker"),
                                index,
                            ) {
                                Ok(label) => {
                                    self.fields[0].editor =
                                        TextBuffer::new(label).unwrap_or_default();
                                    self.dirty = true;
                                    self.error.clear();
                                    self.focus = 0;
                                    self.active_field = 0;
                                    self.metadata_picker = None;
                                    self.preview = false;
                                }
                                Err(error) => self.error = error,
                            }
                            return;
                        }
                        let source = self.metadata_picker.as_ref().unwrap().source().clone();
                        let selected = if let FormPurpose::Pr(intent) = &self.purpose {
                            crate::pr_metadata_base::PrBaseSelection::new(source, index, intent)
                        } else {
                            Err("The original PR form is no longer active".into())
                        };
                        match selected {
                            Ok(selected) => {
                                self.invalidate_plan();
                                self.fields[0].editor =
                                    TextBuffer::new(selected.name().expect("validated Base value"))
                                        .unwrap_or_default();
                                self.base_selection = Some(selected);
                                self.dirty = true;
                                self.error.clear();
                                self.focus = 0;
                                self.active_field = 0;
                                self.metadata_picker = None;
                            }
                            Err(error) => self.error = error,
                        }
                    }
                    Some(crate::pr_metadata_picker::PrMetadataPickerOutcome::Closed) => {
                        self.metadata_picker = None
                    }
                    _ => {}
                }
            }
            FormAction::ErrorScroll(delta) => {
                self.error_scroll = self
                    .error_scroll
                    .saturating_add_signed(*delta as isize)
                    .min(4096)
            }
            FormAction::FieldScroll(delta) => {
                if !self.fields.is_empty() {
                    self.focus = self
                        .active_field
                        .saturating_add_signed(*delta as isize)
                        .min(self.fields.len() - 1);
                    self.active_field = self.focus;
                    self.field_top = self.focus;
                }
            }
            FormAction::Swap => match self.compare_intent() {
                Ok(mut intent) => {
                    intent.swap();
                    self.fields[0].editor = TextBuffer::new(&intent.base.value).unwrap_or_default();
                    self.fields[1].editor = TextBuffer::new(&intent.head.value).unwrap_or_default();
                    if let FormPurpose::Compare(comparison) = &mut self.purpose {
                        comparison.intent = intent;
                        comparison.plan = None;
                    }
                    self.preview = false;
                    self.dirty = true;
                }
                Err(error) => self.error = error,
            },
            FormAction::NewIntent
                if self.actions_publication.is_some() || self.metadata_original() =>
            {
                if self.metadata_original() {
                    self.publication = None;
                    self.metadata_original_lines.clear();
                    self.base_selection = None;
                    if let FormPurpose::PrMetadata(intent) = &mut self.purpose {
                        intent.clear_selection();
                    }
                }
                self.actions_publication = None;
                self.actions_original_lines.clear();
                self.restored_draft = false;
                self.submitted = false;
                self.submit_requested = false;
                self.preview = false;
                self.preview_requested = false;
                self.invalidate_plan();
                self.dirty = true;
                self.focus = self.active_field.min(self.fields.len().saturating_sub(1));
                self.error = "New local draft only • original durable protection remains; Preview and confirm a new intent explicitly. Nothing sent.".into();
            }
            FormAction::NewIntent => {}
            FormAction::Submit
            | FormAction::Close
            | FormAction::OpenPicker(_)
            | FormAction::InspectConversationV17 => {}
            FormAction::RefreshConversationV19 => {}
            FormAction::OpenDispatchRunV7 => {}
        }
    }
    pub fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<FormAction> {
        if !self.submitting || code == KeyCode::Esc {
            self.observe_presentation_input_v4();
        }
        if inline_ui::anchor_open(self) {
            return match code {
                KeyCode::Esc | KeyCode::F(2) => Some(FormAction::InlineAnchor),
                KeyCode::Up => Some(FormAction::InlineAnchorScroll(-1)),
                KeyCode::Down => Some(FormAction::InlineAnchorScroll(1)),
                KeyCode::PageUp => Some(FormAction::InlineAnchorScroll(-10)),
                KeyCode::PageDown => Some(FormAction::InlineAnchorScroll(10)),
                _ => None,
            };
        }
        let control = modifiers.contains(KeyModifiers::CONTROL);
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        if code == KeyCode::Esc {
            return Some(FormAction::Close);
        }
        if self.submitting {
            return None;
        }
        if code == KeyCode::Char('r')
            && !control
            && !modifiers.contains(KeyModifiers::ALT)
            && crate::scoped_pr_collaboration_ui_v17::return_descriptor_v19(self).is_some()
        {
            return Some(FormAction::RefreshConversationV19);
        }
        if code == KeyCode::F(12) && review_ui::local_batch_v10::removal_candidate(self) {
            return Some(FormAction::RemoveLocalReviewCommentV10);
        }
        if code == KeyCode::F(3)
            && matches!(&self.purpose, FormPurpose::Local(intent) if intent.failed_stash_v1.is_some())
        {
            return Some(FormAction::InspectStashV1);
        }
        if code == KeyCode::F(2) && matches!(self.purpose, FormPurpose::PrInline(_)) {
            return Some(FormAction::InlineAnchor);
        }
        if code == KeyCode::F(2)
            && (crate::scoped_pr_collaboration_ui_v17::metadata_base_v32::state(self).is_some()
                || crate::scoped_pr_collaboration_ui_v17::issue_link_v87::state(self).is_some())
        {
            return Some(FormAction::OpenPicker(0));
        }
        if matches!(self.purpose, FormPurpose::PrInline(_)) {
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter if self.focus == 0 => "\n",
                _ => "",
            };
            if let Err(error) = inline_ui::validate_input(self, incoming) {
                self.error = error;
                return None;
            }
        }
        if matches!(self.purpose, FormPurpose::PrThread(_)) && self.focus < self.fields.len() {
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter => "\n",
                _ => "",
            };
            if let Err(error) = thread_ui::validate_input(self, incoming) {
                self.error = error;
                return None;
            }
        }
        if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) && !self.fields.is_empty() {
            return None;
        }
        if matches!(self.purpose, FormPurpose::PrFileCommentV75(_))
            && self.focus < self.fields.len()
        {
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter => "\n",
                _ => "",
            };
            if let Err(reason) = file_comment_ui_v75::validate_input(self, incoming) {
                self.error = reason;
                return None;
            }
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_))
            && self.focus < self.fields.len()
            && !self.preview
        {
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter if self.fields[self.focus].multiline => "\n",
                _ => "",
            };
            if (!incoming.is_empty() || matches!(code, KeyCode::Backspace | KeyCode::Delete))
                && self.fields[self.focus].choices.is_empty()
            {
                if let Err(reason) =
                    crate::scoped_pr_collaboration_ui_v17::validate_input(self, incoming)
                {
                    self.error = reason;
                    return None;
                }
            }
        }
        if matches!(self.purpose, FormPurpose::PrThreadCommentV2(_))
            && self.focus < self.fields.len()
        {
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter => "\n",
                _ => "",
            };
            if let Err(reason) = comment_ui_v2::validate_input(self, incoming) {
                self.error = reason;
                return None;
            }
        }
        if matches!(self.purpose, FormPurpose::PrReviewOperationV3(_)) {
            if self.publication.is_some()
                && (matches!(code, KeyCode::Backspace | KeyCode::Delete)
                    || matches!(code, KeyCode::Char(_))
                        && !control
                        && !modifiers.contains(KeyModifiers::ALT)
                    || code == KeyCode::Enter && self.focus < self.fields.len())
            {
                self.error =
                    "Original operation publication history is read-only; nothing resent".into();
                return None;
            }
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter if self.focus < self.fields.len() => "\n",
                _ => "",
            };
            if let Err(error) = review_operation_ui_v3::validate_input(self, incoming) {
                self.error = error;
                return None;
            }
        }
        if matches!(self.purpose, FormPurpose::PrReviewV2(_)) {
            if self.publication.is_some()
                && (matches!(code, KeyCode::Backspace | KeyCode::Delete)
                    || matches!(code, KeyCode::Char(_))
                        && !control
                        && !modifiers.contains(KeyModifiers::ALT)
                    || code == KeyCode::Enter && self.focus < self.fields.len())
            {
                self.error =
                    "Original review publication history is read-only; nothing resent".into();
                return None;
            }
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter if self.focus < self.fields.len() => "\n",
                _ => "",
            };
            if let Err(error) = review_ui::validate_input(self, incoming) {
                self.error = error;
                return None;
            }
        }
        if matches!(
            self.purpose,
            FormPurpose::ScopedActionsDispatchV7(_) | FormPurpose::ScopedWorkflowEditV9(_)
        ) && self.focus < self.fields.len()
            && !self.preview
        {
            let mut encoded = [0; 4];
            let incoming = match code {
                KeyCode::Char(ch)
                    if !control
                        && !modifiers.contains(KeyModifiers::ALT)
                        && self.fields[self.focus].choices.is_empty() =>
                {
                    ch.encode_utf8(&mut encoded)
                }
                KeyCode::Enter if self.fields[self.focus].multiline => "\n",
                _ => "",
            };
            if !incoming.is_empty() {
                if let Err(reason) = crate::scoped_actions_dispatch_ui_v7::validate_input(
                    self, incoming,
                )
                .and_then(|()| {
                    crate::scoped_actions_workflow_edit_ui_v9::validate_input(self, incoming)
                }) {
                    self.error = reason;
                    return None;
                }
            }
        }
        if code == KeyCode::F(11) && matches!(self.purpose, FormPurpose::ScopedActionsDispatchV7(_))
        {
            return Some(FormAction::OpenDispatchRunV7);
        }
        if code == KeyCode::F(2) && self.full_input_available(self.active_field) {
            return Some(FormAction::FullInput(self.active_field));
        }
        if code == KeyCode::F(4) && self.field_has_picker(self.active_field) {
            return Some(FormAction::OpenPicker(self.active_field));
        }
        if code == KeyCode::F(4)
            && matches!(
                self.purpose,
                FormPurpose::ScopedActionsV3(_)
                    | FormPurpose::ScopedActionsWorkflowV6(_)
                    | FormPurpose::ScopedWorkflowEditV9(_)
            )
        {
            return Some(FormAction::AcknowledgeActionsV3);
        }
        if code == KeyCode::F(6)
            && (matches!(
                self.purpose,
                FormPurpose::ScopedActionsV3(_) | FormPurpose::ScopedActionsWorkflowV6(_)
            ) || crate::scoped_actions_workflow_edit_ui_v9::history_ack_key_v9(self)
                || matches!(self.purpose, FormPurpose::ScopedActionsDispatchV7(_))
                    && self.actions_publication.is_some())
        {
            return Some(FormAction::AcknowledgeActionsAcceptanceV5);
        }
        if code == KeyCode::F(9) && matches!(self.purpose, FormPurpose::Compare(_)) {
            return Some(FormAction::Swap);
        }
        if code == KeyCode::Char('n')
            && !control
            && !modifiers.contains(KeyModifiers::ALT)
            && matches!(&self.purpose,FormPurpose::PrCollaborationV17(s) if s.needs_new_v18())
        {
            return Some(FormAction::NewIntent);
        }
        if code == KeyCode::F(9)
            && matches!(self.purpose, FormPurpose::PrCollaborationV17(_))
            && self.publication.is_some()
        {
            return Some(FormAction::InspectConversationV17);
        }
        if code == KeyCode::F(9) && (self.actions_publication.is_some() || self.metadata_original())
        {
            return Some(FormAction::NewIntent);
        }
        if code == KeyCode::F(7) {
            return Some(FormAction::ErrorScroll(-3));
        }
        if code == KeyCode::F(8) {
            return Some(FormAction::ErrorScroll(3));
        }
        if code == KeyCode::F(5) {
            return Some(FormAction::FieldScroll(-1));
        }
        if code == KeyCode::F(6) {
            return Some(FormAction::FieldScroll(1));
        }
        if control && code == KeyCode::Char('p') {
            return Some(FormAction::Preview);
        }
        if control && code == KeyCode::Char('e') {
            return Some(FormAction::Editor);
        }
        // Terminals rarely deliver Ctrl-Enter, so Ctrl-S is the dependable
        // keyboard equivalent of the confirm/submit button (same action).
        if control && matches!(code, KeyCode::Enter | KeyCode::Char('s')) {
            return Some(FormAction::Submit);
        }
        if (self.preview
            || matches!(
                self.purpose,
                FormPurpose::ScopedActionsV3(_) | FormPurpose::ScopedActionsWorkflowV6(_)
            ))
            && self.uses_prepared_preview()
        {
            match code {
                KeyCode::Down => self.preview_scroll += 1,
                KeyCode::Up => self.preview_scroll = self.preview_scroll.saturating_sub(1),
                KeyCode::PageDown => self.preview_scroll += 10,
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(10),
                _ => {}
            }
        }
        if matches!(code, KeyCode::Tab | KeyCode::BackTab) {
            // FILE Preview hides the body editor. Keyboard traversal must
            // stay on the same visible footer actions as pointer navigation.
            if (self.preview
                || matches!(&self.purpose,FormPurpose::ScopedWorkflowEditV9(state)if state.remote_history_only_v9()))
                && matches!(
                    self.purpose,
                    FormPurpose::PrCollaborationV17(_)
                        | FormPurpose::ScopedActionsV3(_)
                        | FormPurpose::ScopedActionsWorkflowV6(_)
                        | FormPurpose::ScopedActionsDispatchV7(_)
                        | FormPurpose::ScopedWorkflowEditV9(_)
                )
            {
                let count = self.footer_actions().len();
                if count == 0 {
                    return None;
                }
                let current = self
                    .focus
                    .checked_sub(self.fields.len())
                    .filter(|n| *n < count);
                let index = match (code, current) {
                    (KeyCode::Tab, Some(n)) => (n + 1) % count,
                    (KeyCode::BackTab, Some(n)) => (n + count - 1) % count,
                    (KeyCode::BackTab, None) => count - 1,
                    _ => 0,
                };
                self.focus = self.fields.len() + index;
                return None;
            }
            let count = self.fields.len() + self.footer_actions().len();
            self.focus = if self.preview && matches!(self.purpose, FormPurpose::PrFileCommentV75(_))
            {
                let first = self.fields.len();
                let count = count - first;
                let current = self.focus.saturating_sub(first).min(count - 1);
                first
                    + if code == KeyCode::Tab {
                        (current + 1) % count
                    } else {
                        (current + count - 1) % count
                    }
            } else if code == KeyCode::Tab {
                (self.focus + 1) % count
            } else {
                (self.focus + count - 1) % count
            };
            if self.focus < self.fields.len() {
                self.active_field = self.focus;
            }
            return None;
        }
        if crate::scoped_actions_workflow_edit_ui_v9::protect_committed_input_v9(
            self, code, modifiers,
        ) {
            return None;
        }
        let Some(field) = self.fields.get_mut(self.focus) else {
            return if code == KeyCode::Enter {
                self.footer_actions()
                    .get(self.focus - self.fields.len())
                    .cloned()
            } else {
                None
            };
        };
        if code == KeyCode::Char(' ') && !field.choices.is_empty() {
            let next = field
                .choices
                .iter()
                .position(|choice| choice == &field.editor.text)
                .map_or(0, |current| (current + 1) % field.choices.len());
            return Some(FormAction::Choice(self.focus, next));
        }
        if self.preview {
            return None;
        }
        if self.publication.is_some()
            && matches!(self.purpose, FormPurpose::PrThread(_))
            && (matches!(code, KeyCode::Backspace | KeyCode::Delete | KeyCode::Enter)
                || matches!(code, KeyCode::Char('u')) && control
                || matches!(code, KeyCode::Char(_)) && !control)
        {
            self.error =
                "Original thread publication is inspect-only; saved body and protection retained."
                    .into();
            return None;
        }
        let before = field.editor.text.clone();
        let field_key = field.key.clone();
        match code {
            KeyCode::Char('a') if control => field.editor.select_all(),
            KeyCode::Char('u') if control => field.editor.clear(),
            KeyCode::Left => field.editor.horizontal(false, shift),
            KeyCode::Right => field.editor.horizontal(true, shift),
            KeyCode::Up => field.editor.vertical(false, shift),
            KeyCode::Down => field.editor.vertical(true, shift),
            KeyCode::Home => field.editor.line_edge(false, control, shift),
            KeyCode::End => field.editor.line_edge(true, control, shift),
            KeyCode::Backspace => field.editor.delete_backward(),
            KeyCode::Delete => field.editor.delete_forward(),
            KeyCode::Enter if field.multiline => {
                if let Err(error) = field.editor.insert("\n") {
                    self.error = error;
                }
            }
            KeyCode::Char(ch) if !control && !modifiers.contains(KeyModifiers::ALT) => {
                if let Err(error) = field.editor.insert(&ch.to_string()) {
                    self.error = error;
                }
            }
            _ => {}
        }
        let changed = before != field.editor.text;
        self.dirty |= changed;
        if changed {
            self.field_edited(&field_key, &before);
        }
        None
    }
    pub fn paste(&mut self, text: &str) {
        // Same pending/read-only boundary as keyboard editing. A paste that
        // cannot edit must not invalidate an already prepared approval just
        // because the earlier draft still awaits autosave (dirty == true).
        // An in-flight Preview is not yet `preview == true`.
        if self.submitting || self.preview {
            return;
        }
        if let Err(reason) = crate::scoped_actions_workflow_edit_ui_v9::validate_input(self, text) {
            self.error = reason;
            return;
        }
        if matches!(self.purpose, FormPurpose::ScopedActionsDispatchV7(_)) {
            if let Err(reason) = crate::scoped_actions_dispatch_ui_v7::validate_input(self, text) {
                self.error = reason;
                return;
            }
        }
        if matches!(self.purpose, FormPurpose::PrFileCommentV75(_)) {
            if let Err(reason) = file_comment_ui_v75::validate_input(self, text) {
                self.error = reason;
                return;
            }
        }
        if matches!(self.purpose, FormPurpose::PrCollaborationV17(_)) {
            if let Err(reason) = crate::scoped_pr_collaboration_ui_v17::validate_input(self, text) {
                self.error = reason;
                return;
            }
        }
        self.observe_presentation_input_v4();
        if inline_ui::anchor_open(self) {
            return;
        }
        if matches!(self.purpose, FormPurpose::PrInline(_)) && !self.preview {
            if let Err(error) = inline_ui::validate_input(self, text) {
                self.error = error;
                return;
            }
        }
        if matches!(self.purpose, FormPurpose::PrThread(_)) && !self.preview {
            if let Err(error) = thread_ui::validate_input(self, text) {
                self.error = error;
                return;
            }
        }
        if matches!(self.purpose, FormPurpose::PrReviewOperationV3(_)) && !self.preview {
            if let Err(error) = review_operation_ui_v3::validate_input(self, text) {
                self.error = error;
                return;
            }
        }
        if matches!(self.purpose, FormPurpose::PrReviewV2(_)) && !self.preview {
            if let Err(error) = review_ui::validate_input(self, text) {
                self.error = error;
                return;
            }
        }
        if matches!(self.purpose, FormPurpose::PrThreadReactionV3(_)) {
            return;
        }
        if matches!(self.purpose, FormPurpose::PrThreadCommentV2(_)) {
            if let Err(reason) = comment_ui_v2::validate_input(self, text) {
                self.error = reason;
                return;
            }
        }
        if !self.preview {
            if let Some(field) = self.fields.get_mut(self.focus) {
                let before = field.editor.text.clone();
                let key = field.key.clone();
                let text = if field.multiline {
                    text.to_owned()
                } else {
                    text.replace(['\r', '\n'], " ")
                };
                match field.editor.insert(&text) {
                    Ok(()) => self.dirty = true,
                    Err(error) => self.error = error,
                }
                self.field_edited(&key, &before);
            }
        }
        if self.dirty {
            self.invalidate_plan();
        }
    }
    pub fn click(&mut self, column: u16, row: u16, extend: bool) {
        if let Some((index, rect)) = self
            .field_rects
            .iter()
            .enumerate()
            .find(|(_, rect)| rect.contains((column, row).into()))
        {
            self.focus = index;
            self.active_field = index;
            self.fields[index].editor.click(
                row.saturating_sub(rect.y) as usize,
                column.saturating_sub(rect.x) as usize,
                extend,
            );
        }
    }
}

pub(crate) fn editor_line(
    editor: &TextBuffer,
    line: &str,
    start: usize,
    width: usize,
    theme: crate::theme::Theme,
    secret: bool,
) -> Line<'static> {
    let mut spans = Vec::new();
    let mut column = 0;
    let mut visible = 0;
    let selection = editor.selection();
    for (offset, grapheme) in line.grapheme_indices(true) {
        let cells = if secret { 1 } else { grapheme_width(grapheme) };
        if column >= editor.left_column {
            if visible + cells > width {
                break;
            }
            let selected = selection.as_ref().is_some_and(|range| {
                start + offset < range.end && start + offset + grapheme.len() > range.start
            });
            let style = if selected {
                theme.selected(SemanticToken::Normal)
            } else {
                theme.style(SemanticToken::Normal)
            };
            let text = if secret {
                "•".into()
            } else if grapheme == "\t" {
                "    ".into()
            } else {
                safe_text(grapheme)
            };
            spans.push(Span::styled(text, style));
            visible += cells;
        }
        column += cells;
    }
    Line::from(spans)
}

fn draw_prepared_resize_v1(
    frame: &mut Frame,
    form: &mut Form,
    rect: Rect,
    hits: &mut Vec<HitRegion>,
    theme: crate::theme::Theme,
) {
    let dispatch_fields_v7 = (matches!(form.purpose, FormPurpose::ScopedActionsDispatchV7(_))
        && !form.preview
        && form.actions_publication.is_none())
        || crate::scoped_actions_workflow_edit_ui_v9::editable_summary(form);
    if dispatch_fields_v7 {
        crate::scoped_actions_dispatch_ui_v7::begin_draw(form, false);
        crate::scoped_actions_workflow_edit_ui_v9::begin_draw(form, false);
    }
    form.preview_display_v1.geometry_rejected = true;
    form.field_rects.clear();
    form.error_rect = Rect::default();
    hits.clear();
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Preview • resize required ")
            .style(theme.base()),
        rect,
    );
    let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
    let body_height = inner.height.saturating_sub(3);
    let explanation = if matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
        "Resize to inspect the workflow summary and one complete local input. Fields, Preview and Confirm are disabled; Close keeps private text."
    } else if dispatch_fields_v7 {
        "Resize to inspect the Dispatch summary and one complete input. Fields, Preview and Confirm are disabled; Close preserves protection."
    } else if form.error.is_empty() {
        "Resize to inspect64 text cells. Preview is unchanged; Confirm cannot use hidden content. Edit fields or Close remain available."
    } else {
        "Confirm refused • resize to inspect Preview. No hidden confirmation; Edit fields or Close remain available."
    };
    frame.render_widget(
        Paragraph::new(explanation)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(theme.style(SemanticToken::Pending)),
        Rect::new(inner.x, inner.y, inner.width, body_height),
    );
    let controls = [
        ("[Ctrl+P Edit fields]", FormAction::Preview),
        ("[Ctrl+Enter Confirm unavailable]", FormAction::Submit),
        ("[Esc Close]", FormAction::Close),
    ];
    for (index, (label, action)) in controls.into_iter().enumerate() {
        if dispatch_fields_v7 && action != FormAction::Close {
            continue;
        }
        if inner.height < 3 && action != FormAction::Close {
            continue;
        }
        let offset = if inner.height < 3 {
            inner.height.saturating_sub(1)
        } else {
            body_height + index as u16
        };
        if inner.height == 0 {
            continue;
        }
        let target = Rect::new(
            inner.x,
            inner.y + offset,
            (label.len() as u16).min(inner.width),
            1,
        );
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if action == FormAction::Submit {
                SemanticToken::Muted
            } else {
                SemanticToken::Focus
            })),
            target,
        );
        if !target.is_empty() {
            hits.push(HitRegion {
                rect: target,
                action: Action::Form(action),
            });
        }
    }
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if crate::scoped_pr_collaboration_ui_v17::retire_invalid_form(app) {
        return;
    }
    if crate::scoped_actions_publication_ui_v3::retire_invalid_form(app) {
        return;
    }
    if crate::scoped_actions_workflow_edit_ui_v9::retire_invalid_form(app) {
        return;
    }
    if file_comment_ui_v75::retire_invalid_form(app) {
        return;
    }
    if inline_ui::retire_invalid_form(app) {
        return;
    }
    if review_ui::retire_invalid_form(app) {
        return;
    }
    if review_operation_ui_v3::retire_invalid_form(app) {
        return;
    }
    let theme = app.theme();
    let Some(form) = &mut app.form else {
        return;
    };
    if matches!(
        form.purpose,
        FormPurpose::PrReviewV2(_)
            | FormPurpose::PrReviewOperationV3(_)
            | FormPurpose::ScopedActionsV3(_)
            | FormPurpose::ScopedActionsWorkflowV6(_)
            | FormPurpose::ScopedActionsDispatchV7(_)
            | FormPurpose::ScopedWorkflowEditV9(_)
    ) {
        let usable = area.width >= 80 && area.height >= 20;
        review_ui::begin_draw(form, usable);
        review_operation_ui_v3::begin_draw(form, usable);
        crate::scoped_actions_publication_ui_v3::begin_draw(form, usable);
        crate::scoped_actions_workflow_edit_ui_v9::begin_draw(form, usable);
        if !usable {
            if form.preview
                && form.uses_prepared_preview()
                && !matches!(
                    form.purpose,
                    FormPurpose::ScopedActionsV3(_)
                        | FormPurpose::ScopedActionsWorkflowV6(_)
                        | FormPurpose::ScopedActionsDispatchV7(_)
                        | FormPurpose::ScopedWorkflowEditV9(_)
                )
            {
                draw_prepared_resize_v1(frame, form, area, &mut app.hits, theme);
                return;
            }
            form.field_rects.clear();
            form.error_rect = Rect::default();
            app.hits.clear();
            frame.render_widget(Clear, area);
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .title(
                        if matches!(
                            form.purpose,
                            FormPurpose::ScopedActionsV3(_)
                                | FormPurpose::ScopedActionsWorkflowV6(_)
                                | FormPurpose::ScopedActionsDispatchV7(_)
                                | FormPurpose::ScopedWorkflowEditV9(_)
                        ) {
                            " Original Actions • resize required "
                        } else {
                            " Original review • resize required "
                        },
                    )
                    .style(theme.base()),
                area,
            );
            let inner = area.inner(ratatui::layout::Margin::new(1, 1));
            frame.render_widget(
                Paragraph::new(
                    if matches!(
                        form.purpose,
                        FormPurpose::ScopedActionsV3(_)
                            | FormPurpose::ScopedActionsWorkflowV6(_)
                            | FormPurpose::ScopedActionsDispatchV7(_)
                            | FormPurpose::ScopedWorkflowEditV9(_)
                    ) {
                        "Resize to 80×20\nActions disabled until resized"
                    } else {
                        "Resize to80×20 to inspect exact review; actions disabled"
                    },
                )
                .style(theme.style(SemanticToken::Pending))
                .wrap(ratatui::widgets::Wrap { trim: false }),
                inner,
            );
            if inner.height > 3 && !form.error.is_empty() {
                frame.render_widget(
                    Paragraph::new(safe_text(&form.error))
                        .style(theme.style(SemanticToken::Failure)),
                    Rect::new(inner.x, inner.y + 2, inner.width, 1),
                );
            }
            if inner.height > 1 {
                let rect = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(11), 1);
                frame.render_widget(Paragraph::new("[Esc Close]"), rect);
                app.hits.push(HitRegion {
                    rect,
                    action: Action::Form(FormAction::Close),
                });
            }
            return;
        }
    }
    form.retire_changed_current_input();
    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(4).min(28);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    if inline_ui::anchor_open(form) {
        app.hits
            .extend(inline_ui::draw_anchor(frame, form, rect, theme));
        return;
    }
    frame.render_widget(Clear, rect);
    if crate::scoped_pr_collaboration_ui_v17::issue_link_v87::draw_picker(frame, form, rect, theme)
        || crate::scoped_pr_collaboration_ui_v17::metadata_base_v32::draw_picker(
            frame, form, rect, theme,
        )
    {
        app.hits
            .extend(form.metadata_picker_hits.iter().map(|hit| HitRegion {
                rect: hit.rect,
                action: Action::Form(FormAction::MetadataPicker(hit.action.clone())),
            }));
        return;
    }
    if let Some(picker) = &mut form.metadata_picker {
        form.metadata_picker_hits = picker.draw(frame, rect, theme);
        app.hits
            .extend(form.metadata_picker_hits.iter().map(|hit| HitRegion {
                rect: hit.rect,
                action: Action::Form(FormAction::MetadataPicker(hit.action.clone())),
            }));
        return;
    }
    if let Some(picker) = &mut form.picker {
        app.hits.extend(
            picker
                .draw(frame, rect, theme)
                .into_iter()
                .map(|hit| HitRegion {
                    rect: hit.rect,
                    action: Action::Form(FormAction::Picker(hit.action)),
                }),
        );
        return;
    }
    let preview_inner_v1 = rect.inner(ratatui::layout::Margin::new(2, 1));
    if form.preview && form.uses_prepared_preview() && preview_inner_v1.width < 64 {
        draw_prepared_resize_v1(frame, form, rect, &mut app.hits, theme);
        return;
    }
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(format!(" {} ", safe_text(&form.title)))
            .style(theme.base()),
        rect,
    );
    let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
    let footer_height =
        if form.error.is_empty() { 3 } else { 6 } + if form.fields.len() > 1 { 1 } else { 0 };
    let mut y = inner.y;
    let context = match &form.purpose {
        FormPurpose::PrCollaborationV17(state) => state.context(),
        FormPurpose::ScopedActionsV3(state) => state.context(),
        FormPurpose::ScopedActionsWorkflowV6(state) => state.context(),
        FormPurpose::ScopedActionsDispatchV7(state) => state.context(),
        FormPurpose::ScopedWorkflowEditV9(state) => state.context().to_owned(),
        FormPurpose::PrThread(state) => state.context(),
        FormPurpose::PrThreadCommentV2(state) => state.context(),
        FormPurpose::PrThreadReactionV3(state) => state.context(),
        FormPurpose::PrFileCommentV75(state) => state.context(),
        FormPurpose::PrInline(state) => state.context(),
        FormPurpose::PrReviewV2(state) => state.context(),
        FormPurpose::PrReviewOperationV3(state) => state.context(),
        FormPurpose::Pr(intent) => format!(
            "@{} on {} • head {}",
            intent.identity.login, intent.identity.host, intent.target.head_sha
        ),
        // The requester's shared avatar (or its honest `[?]` fallback) is
        // drawn in the first four cells; the login follows it as plain
        // text, never as a bare `@login` (TASK-24/25 AC6).
        FormPurpose::PrAction(intent) => format!(
            "{} on {} • {}#{} • head {}",
            intent.context.identity.login,
            intent.context.identity.host,
            intent.context.target.pr.repository,
            intent.context.target.pr.number,
            intent.context.target.head_sha
        ),
        FormPurpose::PrMetadata(intent) => format!(
            "@{} on {} • numeric account #{} • head {}",
            intent.identity.login,
            intent.identity.host,
            intent.identity.account_id.unwrap_or_default(),
            intent.original.head_sha
        ),
        FormPurpose::Local(intent) => format!(
            "Local {} • {}",
            intent.context.directory.display(),
            intent
                .context
                .status
                .branch
                .as_deref()
                .unwrap_or("detached HEAD")
        ),
        FormPurpose::Actions(intent) => intent.workspace.context(),
        FormPurpose::Corpus(intent) => format!(
            "@{} on {} • selected corpus • {}",
            intent.identity.login,
            intent.identity.host,
            intent.root.display()
        ),
        FormPurpose::Compare(intent) => format!(
            "Local {} • read-only saved versions",
            intent.intent.directory.display()
        ),
    };
    if !matches!(
        form.purpose,
        FormPurpose::PrInline(_)
            | FormPurpose::PrFileCommentV75(_)
            | FormPurpose::PrThread(_)
            | FormPurpose::PrThreadCommentV2(_)
            | FormPurpose::PrThreadReactionV3(_)
    ) {
        frame.render_widget(
            Paragraph::new(safe_text(&context)).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }
    let identity = match &form.purpose {
        FormPurpose::PrCollaborationV17(state) => Some(state.identity()),
        FormPurpose::ScopedActionsV3(state) => Some(state.identity()),
        FormPurpose::ScopedActionsWorkflowV6(state) => Some(state.identity()),
        FormPurpose::ScopedActionsDispatchV7(state) => Some(state.identity()),
        FormPurpose::ScopedWorkflowEditV9(state) => Some(state.identity()),
        FormPurpose::PrThread(state) => Some(state.identity()),
        FormPurpose::PrThreadCommentV2(state) => Some(state.identity()),
        FormPurpose::PrThreadReactionV3(state) => Some(state.identity()),
        FormPurpose::PrFileCommentV75(state) => Some(state.identity()),
        FormPurpose::PrInline(state) => Some(state.identity()),
        FormPurpose::PrReviewV2(state) => Some(state.identity()),
        FormPurpose::PrReviewOperationV3(state) => Some(state.identity()),
        FormPurpose::Pr(intent) => Some(&intent.identity),
        FormPurpose::PrAction(intent) => Some(&intent.context.identity),
        FormPurpose::PrMetadata(intent) => Some(&intent.identity),
        FormPurpose::Actions(intent) => Some(&intent.workspace.identity),
        FormPurpose::Corpus(intent) => Some(&intent.identity),
        FormPurpose::Local(_) | FormPurpose::Compare(_) => None,
    };
    if let Some(identity) = identity {
        if !review_avatars::draw_form_requester(
            frame,
            Rect::new(inner.x, y, 4.min(inner.width), 1),
            form,
            &app.avatar_states,
            theme,
        ) {
            // The avatar cells are cleared first, so a fallback shorter than
            // four cells never leaves a stray context character beside it.
            frame.render_widget(
                Paragraph::new("    ").style(theme.base()),
                Rect::new(inner.x, y, 4.min(inner.width), 1),
            );
            let actor = crate::providers::gh::Actor {
                login: identity.login.clone(),
                ..Default::default()
            };
            let identity = crate::identity::ActorIdentity::github(&identity.host, &actor);
            let image = match app.avatar_states.get(&identity) {
                Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            };
            frame.render_widget(
                crate::identity::AvatarWidget {
                    identity: &identity,
                    image,
                    theme,
                },
                Rect::new(inner.x, y, 4.min(inner.width), 1),
            );
        }
        frame.render_widget(
            Paragraph::new(safe_text(&context)).style(theme.style(SemanticToken::Muted)),
            Rect::new(
                inner.x + 4.min(inner.width),
                y,
                inner.width.saturating_sub(4),
                1,
            ),
        );
    }
    y += 1;
    y += crate::scoped_pr_collaboration_ui_v17::draw_author(
        frame,
        Rect::new(
            inner.x,
            y,
            inner.width,
            inner.bottom().saturating_sub(y + footer_height),
        ),
        form,
        &app.avatar_states,
        theme,
    );
    y += review_avatars::draw_form_author(
        frame,
        Rect::new(
            inner.x,
            y,
            inner.width,
            inner.bottom().saturating_sub(y + footer_height),
        ),
        form,
        &app.avatar_states,
        theme,
    );
    frame.render_widget(
        Paragraph::new(if matches!(form.purpose, FormPurpose::Compare(_)) {
            "READ-ONLY COMPARISON • No checkout, fetch, index or working-file writes"
        } else if form.is_local() {
            if form.preview
                && matches!(&form.purpose, FormPurpose::Local(intent) if intent.pr_create_v1.is_some())
            {
                "READ-ONLY PREVIEW • Confirm action only creates the pull request on GitHub"
            } else if form.preview {
                "READ-ONLY PREVIEW • Confirm action is the only Git write"
            } else {
                "LOCAL INTENT • Preview first, then explicitly confirm"
            }
        } else if form.metadata_original() {
            if matches!(form.purpose, FormPurpose::PrMetadata(_)) {
                "ORIGINAL METADATA RECORD • Read-only; do not resend"
            } else {
                "ORIGINAL BASE RECORD • Viewing or editing never resends it"
            }
        } else if let FormPurpose::PrCollaborationV17(state) = &form.purpose {
            if state.is_terminal_read_v19(){"CONFIRMED RECORD • explicit fresh read only • no publication or local ACK"}
            else if state.is_history() {
                "PROTECTED HISTORY • Inspect current; ACK is local only; never resend"
            } else if form.publication.is_some() {
                "PROTECTED ORIGINAL • Inspect history; never resend or restore approval"
            } else if form.preview {
                "READ-ONLY PREVIEW • Confirm action publishes this exact PR intent"
            } else {
                "PR INTENT • Preview first, then explicitly confirm"
            }
        } else if matches!(
            form.purpose,
            FormPurpose::PrAction(_)
                | FormPurpose::PrMetadata(_)
                | FormPurpose::PrInline(_)
                | FormPurpose::PrThread(_)
                | FormPurpose::PrReviewV2(_)
                | FormPurpose::PrReviewOperationV3(_)
                | FormPurpose::PrThreadCommentV2(_)
                | FormPurpose::PrThreadReactionV3(_)
                | FormPurpose::PrFileCommentV75(_)
        ) || (matches!(form.purpose, FormPurpose::Pr(_)) && form.uses_prepared_preview())
        {
            if form.preview {
                "READ-ONLY PREVIEW • Confirm action publishes this exact PR intent"
            } else {
                "PR INTENT • Preview first, then explicitly confirm"
            }
        } else if matches!(form.purpose, FormPurpose::Actions(_)) {
            if form.actions_publication.is_some() || form.metadata_original() {
                "ORIGINAL ATTEMPT • Edited fields never resend it; inspect the recorded result"
            } else if form.preview {
                "READ-ONLY PREVIEW • Confirm action performs the exact Actions intent"
            } else {
                "ACTIONS INTENT • Preview first, then explicitly confirm"
            }
        } else if matches!(form.purpose, FormPurpose::Corpus(_)) {
            if form.preview {
                "REPOSITORY PREVIEW • Confirm applies only this exact path and effect"
            } else {
                "REPOSITORY INTENT • No automatic clone; preview, then confirm"
            }
        } else if let FormPurpose::ScopedWorkflowEditV9(state)=&form.purpose {
            if state.remote_history_only_v9(){"ORIGINAL WORKFLOW REMOTE HISTORY • READ/ACK only • effect UNVERIFIED • never resend"}
            else if form.actions_publication.is_some(){"ORIGINAL WORKFLOW LOCAL/PR RESULT • inspect; never resend"}
            else if form.preview{"EXACT LOCAL STEP • one explicit Confirm • no automatic retry/rollback"}
            else{"WORKFLOW LOCAL INTENT • Preview each exact step • no effect from editing"}
        } else if let FormPurpose::ScopedActionsDispatchV7(_)=&form.purpose {
            if form.actions_publication.is_some(){crate::scoped_actions_dispatch_ui_v7::result_heading_v42(form)}
            else if form.preview{"DISPATCH PREVIEW • exact ref/protocol/inputs; one explicit Confirm"}
            else{"DISPATCH INTENT • inspect exact schema, then Preview; no POST sent"}
        } else if let FormPurpose::ScopedActionsWorkflowV6(_)=&form.purpose {
            if form.actions_publication.is_some(){crate::scoped_actions_workflow_ui_v6::result_heading_v42(form)}
            else if form.preview{"WHOLE WORKFLOW PREVIEW • explicit Confirm attempts once across all refs"}
            else{"ORIGINAL WORKFLOW INTENT • Preview first; no PUT sent"}
        } else if let FormPurpose::ScopedActionsV3(state)=&form.purpose {
            if form.actions_publication.is_some() {
                "ORIGINAL PROTECTED RESULT • inspect only; never resend"
            } else if matches!(state.opened().intent().command(), crate::actions_workspace::ActionsCommand::ForceCancel
                | crate::actions_workspace::ActionsCommand::DeleteRun | crate::actions_workspace::ActionsCommand::DeleteLogs) {
                if form.preview {"DESTRUCTIVE RUN PREVIEW • exact whole-run effect; explicit Confirm attempts once"}
                else{"ORIGINAL RUN INTENT • Preview before explicit Confirm; no automatic retry"}
            } else if state.opened().intent().command()!=crate::actions_workspace::ActionsCommand::Cancel {
                if form.preview {"RERUN PREVIEW • acceptance is not workflow completion; explicit Confirm attempts once"}
                else{"ORIGINAL RERUN INTENT • exact debug choice; Preview before Confirm"}
            } else if form.preview {
                "CANCEL PREVIEW • nothing sent; explicit Confirm attempts once"
            } else {
                "ORIGINAL CANCEL INTENT • read only; Preview before Confirm"
            }
        } else if form.preview {
            "PREVIEW • Nothing published"
        } else {
            "LOCAL DRAFT • Submit is the only publishing action"
        })
        .style(theme.style(SemanticToken::Pending)),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
    if let Some(effect) = match &form.purpose {
        FormPurpose::Pr(intent) => intent.review_effect(),
        _ => None,
    } {
        frame.render_widget(
            Paragraph::new(effect).style(theme.style(SemanticToken::Normal)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }
    if matches!(form.purpose, FormPurpose::PrInline(_)) {
        let area = Rect::new(inner.x, y, inner.width.min(26), 1);
        frame.render_widget(
            Paragraph::new("[F2 Full anchor details]").style(theme.style(if form.submitting {
                SemanticToken::Muted
            } else {
                SemanticToken::Focus
            })),
            area,
        );
        app.hits.push(HitRegion {
            rect: area,
            action: Action::Form(FormAction::InlineAnchor),
        });
        y += 1;
    }
    if let FormPurpose::PrAction(intent) = &form.purpose {
        if intent.command == crate::pr_actions::PrCommand::Subscription {
            let current = intent.context.current_subscription.as_ref().map_or_else(
                || "Current subscription unavailable • choose no default as fact".into(),
                |snapshot| {
                    format!(
                        "Current subscription: {} • {}",
                        snapshot.state.map_or("unavailable", |state| match state {
                            crate::providers::gh::Subscription::Subscribed => "subscribed",
                            crate::providers::gh::Subscription::Unsubscribed => "unsubscribed",
                            crate::providers::gh::Subscription::Ignored => "ignored",
                        }),
                        if snapshot.viewer_can_subscribe {
                            "changes permitted"
                        } else {
                            "changes not permitted"
                        }
                    )
                },
            );
            frame.render_widget(
                Paragraph::new(current).style(theme.style(SemanticToken::Normal)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
    }
    if let FormPurpose::PrCollaborationV17(state) = &form.purpose {
        if let Some(snapshot) = state.current_subscription() {
            frame.render_widget(
                Paragraph::new(format!(
                    "Current subscription: {} • observed at Open",
                    snapshot.state_label()
                ))
                .style(theme.style(SemanticToken::Normal)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
        if form.preview {
            if let Some(requested) = state.requested_subscription_preview() {
                frame.render_widget(
                    Paragraph::new(format!("Requested subscription state: {requested}"))
                        .style(theme.style(SemanticToken::Normal)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 1;
            }
        }
    }
    let focus_was_visible = form
        .field_rects
        .get(form.focus)
        .is_some_and(|rect| !rect.is_empty());
    if let FormPurpose::Corpus(intent) = &form.purpose {
        let progress = intent.progress.snapshot();
        if form.submitting || progress.phase != crate::search::managed::CorpusPhase::Idle {
            frame.render_widget(
                Paragraph::new(format!(
                    "Clone progress • {:?} • observed {} bytes • {} ms • network total unknown",
                    progress.phase, progress.observed_disk_bytes, progress.elapsed_millis
                ))
                .style(theme.style(SemanticToken::Pending)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
    }
    if form.focus < form.fields.len() && !focus_was_visible {
        form.field_top = form.focus;
    }
    if matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)) {
        // W9's genuine worker reserves all nine fields BEFORE final UID.
        // Drawing must not discard that storage before Commit moves its seed.
        form.field_rects.resize(form.fields.len(), Rect::default());
        form.field_rects.fill(Rect::default());
    } else {
        form.field_rects = vec![Rect::default(); form.fields.len()];
    }
    // Dispatch has multiple paged fields, unlike the Run debug singleton.
    // Its editable summary is one bounded window above those SAME fields;
    // no field may overwrite its summary or create partial Preview authority.
    let dispatch_fields_v7 = (matches!(form.purpose, FormPurpose::ScopedActionsDispatchV7(_))
        && !form.preview
        && form.actions_publication.is_none())
        || crate::scoped_actions_workflow_edit_ui_v9::editable_summary(form);
    if dispatch_fields_v7 && !form.submitting && form.focus < form.fields.len() {
        // Last frame's rectangles cannot prove visibility after a shrink or
        // an added error footer. Reserve the current editable field first;
        // footer focus deliberately retains the user's existing field page.
        form.field_top = form.focus;
    }
    let dispatch_field_rows_v155 = if dispatch_fields_v7
        && !form.submitting
        && form
            .fields
            .get(form.focus)
            .is_some_and(|field| !field.choices.is_empty())
    {
        3 // Label, value and the SAME real indexed-choice mouse actions.
    } else {
        2
    };
    let dispatch_available_v7 = inner.bottom().saturating_sub(y + footer_height);
    if dispatch_fields_v7 && dispatch_available_v7 < dispatch_field_rows_v155 + 1 {
        draw_prepared_resize_v1(frame, form, rect, &mut app.hits, theme);
        return;
    }
    let dispatch_summary_rows_v7 = if dispatch_fields_v7 {
        dispatch_available_v7
            .saturating_sub(dispatch_field_rows_v155)
            .min(4)
    } else {
        0
    };
    if form.uses_prepared_preview()
        && (form.preview
            || crate::scoped_actions_publication_ui_v3::is_original(form)
            || matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_)))
    {
        let body_y_v4 =
            y.saturating_add(crate::scoped_actions_publication_ui_v3::input_rows_v4(form));
        let visible = if dispatch_fields_v7 {
            dispatch_summary_rows_v7
        } else {
            inner.bottom().saturating_sub(body_y_v4 + footer_height)
        } as usize;
        form.preview_display_v1.geometry_rejected = true;
        if !review_avatars::draw_form_preview(
            frame,
            Rect::new(inner.x, body_y_v4, inner.width, visible as u16),
            form,
            &app.avatar_states,
            theme,
        ) {
            let body = Rect::new(inner.x, body_y_v4, inner.width, visible as u16);
            let document = form.preview_display_v1.document.clone();
            // The create-PR preview paints its account and reviewer rows with
            // the shared actor chip; every other preview row stays plain.
            let pr_create_v1 = match &form.purpose {
                FormPurpose::Local(intent) => intent.pr_create_v1.clone(),
                _ => None,
            };
            // PR action previews (merge, auto-merge, queue, lifecycle) paint
            // their actor rows with the same chip and their merge-state rows
            // with the shared semantic ink (TASK-24/25 AC6).
            let pr_action_paint = match &form.purpose {
                FormPurpose::PrAction(intent) => {
                    intent.plan.as_ref().map(|plan| plan.paint.clone())
                }
                // The native State composer (Close/Reopen from Summary and
                // Conversation) paints its account row the same way.
                FormPurpose::PrCollaborationV17(_) => {
                    crate::scoped_pr_collaboration_ui_v17::state_v68::account_paint_v25(form)
                        .or_else(|| {
                            crate::scoped_pr_collaboration_ui_v17::state_v68::history_v74::paint_v25(
                                form,
                            )
                        })
                }
                _ => None,
            };
            let mut summary_viewport_v7 =
                crate::prepared_form_text_v1::FormPreviewViewportV1::default();
            let window = document.as_ref().map(|document| {
                document.window(
                    if dispatch_fields_v7 {
                        &mut summary_viewport_v7
                    } else {
                        &mut form.preview_display_v1.viewport
                    },
                    body.width,
                    body.height,
                    if dispatch_fields_v7 {
                        0
                    } else {
                        form.preview_scroll
                    },
                )
            });
            match window {
                Some(Ok(window)) if window.total_rows()>0 => {
                    if !dispatch_fields_v7 {
                        form.preview_scroll=window.top();
                    }
                    form.preview_display_v1.geometry_rejected=false;
                    for (offset,row) in window.rows().enumerate() {
                        let line = match (&pr_create_v1, &pr_action_paint) {
                            (Some(plan), _) => plan.preview_row_line(row.original_row(), row.first_segment(), row.text(), theme),
                            (None, Some(paint)) => paint.row_line(row.original_row(), row.first_segment(), row.text(), theme),
                            (None, None) => ratatui::text::Line::raw(row.text()),
                        };
                        frame.render_widget(Paragraph::new(line).style(theme.base()),
                            Rect::new(body.x,body.y+offset as u16,body.width,1));
                    }
                }
                _=>frame.render_widget(Paragraph::new(form.preview_display_v1.failure.map_or(
                    "Resize to inspect64 text cells, or choose Preview again • no hidden confirmation",|error|error.message()
                )).style(theme.style(SemanticToken::Pending)),body),
            }
        }
        let useful_preview_v1 = !dispatch_fields_v7
            && !form.preview_display_v1.geometry_rejected
            && visible > 0
            && inner.width >= 68;
        if dispatch_fields_v7 {
            // A partial summary is display only, never a usefully read full
            // Preview or a reused approval marker after returning to fields.
            form.preview_display_v1.geometry_rejected = true;
            y = y.saturating_add(dispatch_summary_rows_v7);
        }
        review_ui::painted_preview(form, useful_preview_v1);
        review_operation_ui_v3::painted_preview(form, useful_preview_v1);
        crate::scoped_pr_collaboration_ui_v17::painted_preview(form, useful_preview_v1);
        crate::scoped_actions_publication_ui_v3::painted_preview(form, useful_preview_v1);
        crate::scoped_actions_workflow_edit_ui_v9::painted_preview(form, useful_preview_v1);
    }
    let full_inputs = (0..form.fields.len())
        .map(|index| form.full_input_available(index))
        .collect::<Vec<_>>();
    let pickers = (0..form.fields.len())
        .map(|index| form.field_has_picker(index))
        .collect::<Vec<_>>();
    let prepared_preview = form.uses_prepared_preview();
    for (index, field) in form.fields.iter_mut().enumerate().skip(form.field_top) {
        if form.preview && prepared_preview {
            break;
        }
        if y >= inner.bottom().saturating_sub(footer_height + 1) {
            break;
        }
        frame.render_widget(
            Paragraph::new(format!("{}:", field.label)).style(theme.style(SemanticToken::Actor)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        if pickers[index] {
            let label = "[F4 Choose]";
            let button = Rect::new(
                inner.right().saturating_sub(12).max(inner.x),
                y,
                12.min(inner.width),
                1,
            );
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Reference)),
                button,
            );
            app.hits.push(HitRegion {
                rect: button,
                action: Action::Form(FormAction::OpenPicker(index)),
            });
        }
        if full_inputs[index] {
            let button = Rect::new(
                inner.right().saturating_sub(30).max(inner.x),
                y,
                16.min(inner.width),
                1,
            );
            frame.render_widget(
                Paragraph::new("[F2 Full input]").style(theme.style(SemanticToken::Reference)),
                button,
            );
            if !button.is_empty() {
                app.hits.push(HitRegion {
                    rect: button,
                    action: Action::Form(FormAction::FullInput(index)),
                });
            }
        }
        y += 1;
        let height = if field.multiline {
            inner.bottom().saturating_sub(y + footer_height + 1).max(1)
        } else {
            1
        };
        let field_area = Rect::new(inner.x, y, inner.width, height);
        form.field_rects[index] = field_area;
        if form.focus == index && !form.preview {
            field
                .editor
                .ensure_visible(field_area.width as usize, field_area.height as usize);
        }
        let mut line_start = 0;
        let lines = field
            .editor
            .text
            .split('\n')
            .map(|line| {
                let start = line_start;
                line_start += line.len() + 1;
                (line, start)
            })
            .skip(field.editor.top_line)
            .take(height as usize)
            .map(|(line, start)| {
                editor_line(
                    &field.editor,
                    line,
                    start,
                    inner.width as usize,
                    theme,
                    field.secret,
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines).style(theme.style(if form.focus == index {
                SemanticToken::Focus
            } else {
                SemanticToken::Normal
            })),
            field_area,
        );
        if !form.preview && form.focus == index {
            let (row, column) = field.editor.position();
            frame.set_cursor_position((
                field_area.x
                    + column
                        .saturating_sub(field.editor.left_column)
                        .min(field_area.width.saturating_sub(1) as usize)
                        as u16,
                field_area.y
                    + row
                        .saturating_sub(field.editor.top_line)
                        .min(height.saturating_sub(1) as usize) as u16,
            ));
        }
        app.hits.push(HitRegion {
            rect: field_area,
            action: Action::Form(FormAction::Field(index)),
        });
        y += height;
        if !field.choices.is_empty() && y < inner.bottom().saturating_sub(footer_height) {
            let mut x = inner.x;
            if !field.choice_labels.is_empty() {
                // Long labels can exceed a narrow row; every option remains
                // reachable by clicking this same indexed action or pressing Space.
                let next = field
                    .choices
                    .iter()
                    .position(|value| value == &field.editor.text)
                    .map_or(0, |current| (current + 1) % field.choices.len());
                let label = "[Space Next]";
                let width = (label.len() as u16).min(inner.width);
                let rect = Rect::new(x, y, width, 1);
                frame.render_widget(
                    Paragraph::new(label).style(theme.style(SemanticToken::Reference)),
                    rect,
                );
                app.hits.push(HitRegion {
                    rect,
                    action: Action::Form(FormAction::Choice(index, next)),
                });
                x = x.saturating_add(width + 1);
            }
            for (choice, value) in field.choices.iter().enumerate() {
                let label = format!(
                    "[{}]",
                    safe_text(field.choice_label(choice)).replace('\n', " ")
                );
                let label_width = grapheme_width(&label).min(u16::MAX as usize) as u16;
                let remaining = inner.right().saturating_sub(x);
                if !field.choice_labels.is_empty() && label_width > remaining {
                    continue;
                }
                let width = label_width.min(remaining);
                let rect = Rect::new(x, y, width, 1);
                frame.render_widget(
                    Paragraph::new(label).style(if field.editor.text == *value {
                        theme.selected(SemanticToken::Reference)
                    } else {
                        theme.style(SemanticToken::Reference)
                    }),
                    rect,
                );
                if width > 0 {
                    app.hits.push(HitRegion {
                        rect,
                        action: Action::Form(FormAction::Choice(index, choice)),
                    });
                }
                x = x.saturating_add(width + 1);
            }
            if !field.choice_labels.is_empty() {
                // Show the selected identity's human label even when that option
                // is reached by cycling beyond the visible direct-choice buttons.
                if let Some(choice) = field
                    .choices
                    .iter()
                    .position(|value| value == &field.editor.text)
                {
                    let heading = Rect::new(
                        inner.x,
                        field_area.y.saturating_sub(1),
                        if pickers[index] {
                            inner.width.saturating_sub(13)
                        } else {
                            inner.width
                        },
                        1,
                    );
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{} • {}",
                            safe_text(&field.label),
                            safe_text(field.choice_label(choice))
                        ))
                        .style(theme.style(SemanticToken::Head)),
                        heading,
                    );
                }
            }
            y += 1;
        }
    }
    let error_area = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(footer_height),
        inner.width,
        footer_height.saturating_sub(2),
    );
    form.error_rect = error_area;
    let failed_stash_available =
        matches!(&form.purpose, FormPurpose::Local(intent) if intent.failed_stash_v1.is_some());
    let outcome_feedback_v33 = crate::scoped_actions_workflow_ui_v6::feedback_v25(form)
        .map(|feedback| (feedback.token(), feedback.label()))
        .or_else(|| {
            crate::scoped_actions_dispatch_ui_v7::feedback_v33(form)
                .map(|feedback| (feedback.token(), feedback.label()))
        });
    let feedback_token_v25 =
        outcome_feedback_v33.map_or(SemanticToken::Failure, |(token, _)| token);
    let feedback_text_v25 = outcome_feedback_v33.map_or_else(
        || safe_text(&form.error),
        |(_, label)| format!("{} • {}", label, safe_text(&form.error)),
    );
    frame.render_widget(
        Paragraph::new(feedback_text_v25)
            .style(theme.style(feedback_token_v25))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((form.error_scroll.min(u16::MAX as usize) as u16, 0)),
        error_area,
    );
    frame.render_widget(
        Paragraph::new(if failed_stash_available {
            "[F3 Inspect stash result] • read-only; not retry/drop approval"
        } else if form.actions_publication.is_some()
            && matches!(
                form.purpose,
                FormPurpose::ScopedActionsWorkflowV6(_) | FormPurpose::ScopedActionsDispatchV7(_)
            )
        {
            "Tab: buttons • Esc: close"
        } else if form.error.is_empty() {
            "Tab: controls • Ctrl-A: select all • Ctrl-P: preview • Ctrl-S: confirm • Esc: keep draft"
        } else {
            "Tab: controls • Esc: keep draft"
        }),
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
    );
    if failed_stash_available {
        app.hits.push(HitRegion {
            rect: Rect::new(
                inner.x,
                inner.bottom().saturating_sub(2),
                inner.width.min(25),
                1,
            ),
            action: Action::Form(FormAction::InspectStashV1),
        });
    }
    let mut x = inner.x;
    let y = inner.bottom().saturating_sub(1);
    let default_controls = vec![
        ("[Preview]", FormAction::Preview),
        ("[External editor]", FormAction::Editor),
        (
            if form.actions_publication.is_some() || form.metadata_original() {
                "[View original]"
            } else if matches!(form.purpose, FormPurpose::Compare(_)) {
                "[Open comparison]"
            } else if form.requires_preview() {
                "[Confirm action]"
            } else {
                "[Submit]"
            },
            FormAction::Submit,
        ),
        (
            if form.submitting {
                "[Esc Cancel]"
            } else {
                "[Keep draft / Close]"
            },
            FormAction::Close,
        ),
    ];
    let controls = crate::scoped_pr_collaboration_ui_v17::footer(form)
        .or_else(|| crate::scoped_actions_workflow_edit_ui_v9::footer(form))
        .or_else(|| crate::scoped_actions_publication_ui_v3::footer(form))
        .unwrap_or(default_controls);
    for (offset, (label, action)) in controls
    .into_iter()
    .filter(|(_, action)| {
        *action != FormAction::Editor
            || !matches!(&form.purpose,FormPurpose::PrCollaborationV17(state)
                if !state.editable_body())
                &&
                !matches!(&form.purpose, FormPurpose::PrThread(state) if form.publication.is_some() || state.intent().action() != crate::scoped_pr_observed_tabs_v1::resources::threads_v1::publication_v1::ThreadPublicationActionV1::Reply)
                &&!matches!(&form.purpose,FormPurpose::PrThreadCommentV2(state)if form.publication.is_some()||state.intent().action!=crate::pr_write_journal::thread_comment_history_v2::CommentActionV2::Edit)
                &&!matches!(&form.purpose,FormPurpose::PrReviewOperationV3(state)if form.publication.is_some()||!state.operation_intent().action().needs_body())
                &&!matches!(form.purpose,FormPurpose::PrThreadReactionV3(_))
                &&!matches!(form.purpose,FormPurpose::PrFileCommentV75(_) if form.publication.is_some())
    })
    .enumerate()
    {
        let width = (label.len() as u16).min(inner.right().saturating_sub(x));
        let area = Rect::new(x, y, width, 1);
        let disabled = (form.submitting && action != FormAction::Close)
            ||(action==FormAction::RefreshConversationV19&&!crate::scoped_pr_collaboration_ui_v17::return_ready_v19(form))
            ||(action==FormAction::NewIntent&&matches!(&form.purpose,FormPurpose::PrCollaborationV17(state) if state.needs_new_v18())&&!crate::scoped_pr_collaboration_ui_v17::new_action_ready_v18(form))
            ||(action==FormAction::AcknowledgeActionsV3&&!crate::scoped_actions_publication_ui_v3::acknowledgement_ready(form))
            ||(action==FormAction::AcknowledgeActionsAcceptanceV5&&!crate::scoped_actions_publication_ui_v3::acceptance_ready_v5(form))
            ||(action==FormAction::OpenDispatchRunV7&&!crate::scoped_actions_dispatch_ui_v7::run_display_ready_v42(form))
            || (action == FormAction::Submit && form.confirmation_blocked_reason().is_some());
        let token = if disabled {
            SemanticToken::Muted
        } else {
            SemanticToken::Focus
        };
        let style = if form.focus == form.fields.len() + offset {
            theme.selected(token)
        } else {
            theme.style(token)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                style.add_modifier(Modifier::BOLD),
            ))),
            area,
        );
        if width > 0 {
            app.hits.push(HitRegion {
                rect: area,
                action: Action::Form(action),
            });
        }
        x += width + 1;
    }
    if !form.error.is_empty() {
        let (up, down) = if outcome_feedback_v33.is_some() {
            ("[F7 Info↑]", "[F8 Info↓]")
        } else {
            ("[F7 Error↑]", "[F8 Error↓]")
        };
        for (offset, label, action) in [
            (0, up, FormAction::ErrorScroll(-3)),
            (13, down, FormAction::ErrorScroll(3)),
        ] {
            let area = Rect::new(
                inner.x + 36.min(inner.width) + offset.min(inner.width.saturating_sub(36)),
                inner.bottom().saturating_sub(2),
                12.min(inner.width.saturating_sub(36 + offset)),
                1,
            );
            frame.render_widget(
                Paragraph::new(label).style(
                    if form
                        .footer_actions()
                        .iter()
                        .position(|candidate| candidate == &action)
                        .is_some_and(|offset| form.focus == form.fields.len() + offset)
                    {
                        theme.selected(feedback_token_v25)
                    } else {
                        theme.style(feedback_token_v25)
                    },
                ),
                area,
            );
            if area.width > 0 {
                app.hits.push(HitRegion {
                    rect: area,
                    action: Action::Form(action),
                });
            }
        }
    }
    if form.fields.len() > 1
        && !(matches!(form.purpose, FormPurpose::ScopedActionsDispatchV7(_))
            && form.actions_publication.is_some())
    {
        for (offset, label, action) in [
            (0, "[F5 Fields↑]", FormAction::FieldScroll(-1)),
            (15, "[F6 Fields↓]", FormAction::FieldScroll(1)),
        ] {
            let area = Rect::new(
                inner.x + 33.min(inner.width) + offset.min(inner.width.saturating_sub(33)),
                inner.bottom().saturating_sub(3),
                14.min(inner.width.saturating_sub(33 + offset)),
                1,
            );
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
                area,
            );
            if area.width > 0 {
                app.hits.push(HitRegion {
                    rect: area,
                    action: Action::Form(action),
                });
            }
        }
    }
    // This is a LOCAL draft operation, deliberately separate from Confirm or
    // any GitHub delete. It is clickable only beside the ACTUALLY visible
    // retained-comment selection; tiny/Preview/busy/protected modes paint none.
    if review_ui::local_batch_v10::removal_candidate(form) {
        let label = "[F12 Remove local comment]";
        let target = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(3),
            label.len() as u16,
            1,
        );
        if target.right() <= inner.right() && review_ui::local_batch_v10::mark_removal_painted(form)
        {
            let selected = form
                .footer_actions()
                .iter()
                .position(|action| *action == FormAction::RemoveLocalReviewCommentV10)
                .is_some_and(|offset| form.focus == form.fields.len() + offset);
            frame.render_widget(
                Paragraph::new(label).style(if selected {
                    theme.selected(SemanticToken::Focus)
                } else {
                    theme.style(SemanticToken::Focus)
                }),
                target,
            );
            app.hits.push(HitRegion {
                rect: target,
                action: Action::Form(FormAction::RemoveLocalReviewCommentV10),
            });
        }
    }
    if matches!(form.purpose, FormPurpose::Compare(_)) {
        let rect = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(3),
            15.min(inner.width),
            1,
        );
        frame.render_widget(
            Paragraph::new("[F9 Swap A/B]").style(theme.style(SemanticToken::Reference)),
            rect,
        );
        if !rect.is_empty() {
            app.hits.push(HitRegion {
                rect,
                action: Action::Form(FormAction::Swap),
            });
        }
    }
    if !matches!(
        form.purpose,
        FormPurpose::ScopedActionsV3(_)
            | FormPurpose::ScopedActionsWorkflowV6(_)
            | FormPurpose::ScopedActionsDispatchV7(_)
    ) && (form.actions_publication.is_some() || form.metadata_original())
    {
        let rect = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(3),
            16.min(inner.width),
            1,
        );
        let style = if form.submitting {
            theme.style(SemanticToken::Muted)
        } else if form.focus == form.fields.len() + form.footer_actions().len() - 1 {
            theme.selected(SemanticToken::Reference)
        } else {
            theme.style(SemanticToken::Reference)
        };
        frame.render_widget(Paragraph::new("[F9 New intent]").style(style), rect);
        if !rect.is_empty() {
            app.hits.push(HitRegion {
                rect,
                action: Action::Form(FormAction::NewIntent),
            });
        }
    }
}

#[cfg(test)]
#[path = "forms_actions_draft_tests.rs"]
mod actions_draft_tests;

#[cfg(test)]
#[path = "forms_metadata_preview_tests.rs"]
mod metadata_preview_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, widgets::Widget};
    #[test]
    fn local_confirm_shortcut_ctrl_s_is_the_submit_button_action_without_typing() {
        use crate::comparison::{CompareEndpoint, CompareIntent, ComparisonMode};
        let mut form = Form::compare(CompareIntent {
            directory: "/fixture/local".into(),
            base: CompareEndpoint::manual("a".repeat(40)),
            head: CompareEndpoint::manual("b".repeat(40)),
            mode: ComparisonMode::Snapshot,
        })
        .unwrap();
        form.dispatch(&FormAction::Field(0));
        let before = form.values();
        for code in [KeyCode::Char('s'), KeyCode::Enter] {
            assert_eq!(
                form.key(code, KeyModifiers::CONTROL),
                Some(FormAction::Submit)
            );
        }
        assert_eq!(form.values(), before, "Ctrl-S never inserts text");
        assert_ne!(
            form.key(KeyCode::Char('s'), KeyModifiers::NONE),
            Some(FormAction::Submit)
        );
    }
    #[test]
    fn compare_commits_form_preserves_pinned_drafts_swaps_and_opens_without_write_confirmation() {
        use crate::comparison::{CompareEndpoint, CompareIntent, ComparisonMode};
        let intent = CompareIntent {
            directory: "/fixture/local".into(),
            base: CompareEndpoint::manual("a".repeat(40)),
            head: CompareEndpoint::manual("b".repeat(40)),
            mode: ComparisonMode::Snapshot,
        };
        let mut form = Form::compare(intent.clone()).unwrap();
        form.choose_local(crate::local_picker::LocalChoice {
            field: "base".into(),
            label: "topic".into(),
            detail: "immutable".into(),
            editor_text: "c".repeat(40),
            value: crate::local_picker::LocalValue::Reference {
                name: "refs/heads/topic".into(),
                id: "c".repeat(40),
            },
            disabled_reason: None,
        })
        .unwrap();
        form.dispatch(&FormAction::Choice(2, 1));
        let draft = form.draft();
        let mut restored = Form::compare(intent).unwrap();
        restored.restore_draft(&draft).unwrap();
        assert_eq!(
            restored.compare_intent().unwrap().mode,
            ComparisonMode::ProposedChanges
        );
        assert!(restored.compare_intent().unwrap().base.selection.is_some());
        restored.dispatch(&FormAction::Swap);
        assert_eq!(restored.values()["base"], "b".repeat(40));
        assert_eq!(restored.values()["head"], "c".repeat(40));
        assert!(restored.compare_intent().unwrap().head.selection.is_some());
        restored.dispatch(&FormAction::Field(1));
        restored.key(KeyCode::Left, KeyModifiers::NONE);
        assert!(restored.compare_intent().unwrap().head.selection.is_some());
        restored.paste("changed");
        assert!(restored.compare_intent().unwrap().head.selection.is_none());
        assert!(!restored.requires_preview());
        restored.dispatch(&FormAction::Submit);
        assert!(restored.submit_requested);
        assert!(restored.error.is_empty());
        form.dispatch(&FormAction::Field(0));
        let mut app = App {
            form: Some(form),
            overlay: Some(crate::app::Overlay::Form),
            ..Default::default()
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Open comparison"));
        assert!(!text.contains("Confirm action"));
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Swap)));
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::OpenPicker(0))));
    }
    fn local_form(command: crate::local_actions::LocalCommand) -> Form {
        Form::local(
            command,
            crate::local_actions::LocalContext {
                directory: std::path::PathBuf::from("/fixture/local"),
                revision: "a".repeat(40),
                status: crate::git::RepoStatus {
                    head: Some("a".repeat(40)),
                    branch: Some("main".into()),
                    ..Default::default()
                },
                selected_path: None,
                comparison: None,
                selected_anchor: None,
                selected_lines: vec![],
            },
        )
        .unwrap()
    }
    #[test]
    fn conversation_comments_unset_choice_space_and_mouse_select_first_explicit_value() {
        use crossterm::event::{Event, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
        for mouse in [false, true] {
            let mut form = local_form(crate::local_actions::LocalCommand::CreateBranch);
            let mut field =
                Field::text("subscription", "Requested subscription state", "", false).unwrap();
            field.choices = vec!["subscribed".into(), "unsubscribed".into(), "ignored".into()];
            field.choice_labels = vec![
                "subscribe to all future conversation activity".into(),
                "only participation or mentions".into(),
                "never notify this account".into(),
            ];
            form.fields = vec![field];
            let mut app = App {
                form: Some(form),
                overlay: Some(crate::app::Overlay::Form),
                ..Default::default()
            };
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            assert_eq!(app.form.as_ref().unwrap().values()["subscription"], "");
            if mouse {
                let next = app
                    .hits
                    .iter()
                    .find(|hit| hit.action == Action::Form(FormAction::Choice(0, 0)))
                    .unwrap()
                    .rect;
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: next.x,
                    row: next.y,
                    modifiers: KeyModifiers::NONE,
                }));
            } else {
                app.handle_event(Event::Key(KeyEvent::new(
                    KeyCode::Char(' '),
                    KeyModifiers::NONE,
                )));
            }
            let current = app.form.as_ref().unwrap();
            assert_eq!(current.values()["subscription"], "subscribed");
            assert!(current.dirty);
            assert!(!current.submit_requested);
            let mut current = current.clone();
            let next = current.key(KeyCode::Char(' '), KeyModifiers::NONE).unwrap();
            assert_eq!(next, FormAction::Choice(0, 1));
            current.dispatch(&next);
            assert_eq!(current.values()["subscription"], "unsubscribed");
        }
    }
    #[test]
    fn conversation_comments_choice_labels_never_replace_canonical_values_and_narrow_cycle_is_clickable(
    ) {
        use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
        let mut form = local_form(crate::local_actions::LocalCommand::CreateBranch);
        let mut field = Field::text("reaction", "Your reaction", "111", false).unwrap();
        field.choices = vec!["111".into(), "222".into(), "333".into()];
        field.choice_labels = vec![
            "My 👍 by @alice #111".into(),
            "My ❤️ by @alice #222".into(),
            "My 🚀 by @alice #333".into(),
        ];
        form.fields = vec![field];
        for mouse in [false, true] {
            let mut app = App {
                form: Some(form.clone()),
                overlay: Some(crate::app::Overlay::Form),
                ..App::default()
            };
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let next = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Form(FormAction::Choice(0, 1)))
                .unwrap()
                .clone();
            assert!(next.rect.width > 0);
            if mouse {
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: next.rect.x,
                    row: next.rect.y,
                    modifiers: KeyModifiers::NONE,
                }));
            } else {
                app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
                    KeyCode::Char(' '),
                    KeyModifiers::NONE,
                )));
            }
            let current = app.form.as_ref().unwrap();
            assert_eq!(current.values()["reaction"], "222");
            assert!(current.dirty);
            let draft: serde_json::Value = serde_json::from_str(&current.draft()).unwrap();
            assert_eq!(draft["values"]["reaction"], "222");
            assert!(!current.draft().contains("@alice"));
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let heading_y = app.form.as_ref().unwrap().field_rects[0].y - 1;
            let buffer = terminal.backend().buffer();
            let heading = buffer
                .content()
                .chunks(buffer.area.width as usize)
                .nth(heading_y as usize)
                .unwrap();
            let heading_text = heading.iter().map(|cell| cell.symbol()).collect::<String>();
            // Wide emoji have a continuation cell; inspect its actual grapheme
            // and the selected canonical label on this header, not a flat buffer.
            assert!(heading.iter().any(|cell| cell.symbol() == "❤️"));
            assert!(heading_text.contains("Your reaction • My"));
            assert!(
                heading_text.contains("@alice #222"),
                "Selected header: {heading_text}"
            );
            assert!(app
                .hits
                .iter()
                .any(|hit| hit.action == Action::Form(FormAction::Choice(0, 2))));
        }
    }
    #[test]
    fn local_picker_drafts_preserve_identity_on_focus_and_remove_it_only_on_actual_edits() {
        let command = crate::local_actions::LocalCommand::SwitchBranch;
        let mut form = local_form(command);
        let value = crate::local_picker::LocalValue::Reference {
            name: "refs/heads/review".into(),
            id: "b".repeat(40),
        };
        form.choose_local(crate::local_picker::LocalChoice {
            field: "name".into(),
            label: "review".into(),
            detail: "exact saved target".into(),
            editor_text: "review".into(),
            value: value.clone(),
            disabled_reason: None,
        })
        .unwrap();
        let draft = form.draft();
        let mut restored = local_form(command);
        restored.restore_draft(&draft).unwrap();
        restored.dispatch(&FormAction::Field(0));
        restored.key(KeyCode::Left, KeyModifiers::empty());
        restored.key(KeyCode::Tab, KeyModifiers::empty());
        let FormPurpose::Local(intent) = &restored.purpose else {
            panic!("local form")
        };
        assert_eq!(intent.selections.get("name"), Some(&value));
        restored.dispatch(&FormAction::Field(0));
        restored.paste("-edited");
        let FormPurpose::Local(intent) = &restored.purpose else {
            panic!("local form")
        };
        assert!(intent.selections.is_empty());
        assert!(intent.plan.is_none());
        restored.restore_draft(r#"{"name":"legacy"}"#).unwrap();
        assert_eq!(restored.values()["name"], "legacy");
        assert!(restored
            .restore_draft(r#"{"version":2,"values":{"name":"wrong"}}"#)
            .is_err());
        assert_eq!(restored.values()["name"], "legacy");
        let mut malformed: serde_json::Value = serde_json::from_str(&draft).unwrap();
        malformed["values"]["name"] = serde_json::json!("another-branch");
        assert!(restored.restore_draft(&malformed.to_string()).is_err());
        assert_eq!(restored.values()["name"], "legacy");
    }
    #[test]
    fn actions_secrets_vars_form_redacts_debug_drafts_restore_and_external_editor() {
        let identity = HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        };
        let workspace =
            crate::actions_workspace::ActionsWorkspace::new(identity, vec!["example/repo".into()]);
        let mut form = Form::actions(
            workspace,
            crate::actions_workspace::ActionsCommand::SetSecret,
        )
        .unwrap();
        let index = form.fields.iter().position(|field| field.secret).unwrap();
        form.fields[index].editor = TextBuffer::new("private-value-never-persist").unwrap();
        assert!(!format!("{form:?}").contains("private-value-never-persist"));
        assert!(!form.draft().contains("private-value-never-persist"));
        form.active_field = index;
        form.dispatch(&FormAction::Editor);
        assert!(!form.editor_requested);
        assert!(form.error.contains("transient"));
        form.fields[index].editor.clear();
        let key = form.fields[index].key.clone();
        form.restore_draft(&serde_json::json!({key:"injected secret"}).to_string())
            .unwrap();
        assert!(form.fields[index].editor.text.is_empty());
    }
    #[test]
    fn local_picker_form_viewport_exposes_each_focused_field_and_shared_picker_button() {
        let mut form = local_form(crate::local_actions::LocalCommand::CreateBranch);
        for index in 0..12 {
            form.fields.push(
                Field::text(
                    &format!("extra{index}"),
                    &format!("Extra {index}"),
                    "",
                    false,
                )
                .unwrap(),
            );
        }
        let mut app = App {
            form: Some(form),
            overlay: Some(crate::app::Overlay::Form),
            ..Default::default()
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        for index in 0..app.form.as_ref().unwrap().fields.len() {
            app.form
                .as_mut()
                .unwrap()
                .dispatch(&FormAction::Field(index));
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            assert!(
                !app.form.as_ref().unwrap().field_rects[index].is_empty(),
                "field {index} is inaccessible"
            );
            assert!(app
                .hits
                .iter()
                .any(|hit| hit.action == Action::Form(FormAction::Field(index))));
            if index == 1 {
                assert!(app
                    .hits
                    .iter()
                    .any(|hit| hit.action == Action::Form(FormAction::OpenPicker(1))));
            }
        }
    }
    #[test]
    fn composer_selection_is_visible_and_secret_preview_never_reveals_text() {
        let mut editor = TextBuffer::new("a界b").unwrap();
        editor.line_edge(false, true, false);
        editor.horizontal(true, false);
        editor.horizontal(true, true);
        let theme = crate::theme::Theme::from_settings(0, false);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 1));
        Paragraph::new(editor_line(&editor, &editor.text, 0, 10, theme, false))
            .render(buffer.area, &mut buffer);
        assert_eq!(buffer[(1, 0)].symbol(), "界");
        assert!(buffer[(1, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(3, 0)].modifier.contains(Modifier::REVERSED));
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 1));
        Paragraph::new(editor_line(&editor, &editor.text, 0, 10, theme, true))
            .render(buffer.area, &mut buffer);
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("•••"));
        assert!(!text.contains('界'));
        assert!(!text.contains('a'));
    }
}
