//! Shared native input dialogs. Editing, previewing and closing never publish.
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
#[derive(Clone, Debug)]
pub struct LocalIntent {
    pub command: crate::local_actions::LocalCommand,
    pub context: crate::local_actions::LocalContext,
    pub plan: Option<crate::local_actions::LocalPlan>,
    pub selections: BTreeMap<String, crate::local_picker::LocalValue>,
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
                &if self.secret {
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
            _ => ("Body", String::new(), true),
        };
        let title = format!(
            "{} • {}#{}",
            intent.kind.label(),
            intent.target.pr.repository,
            intent.target.pr.number
        );
        Ok(Self {
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
            title: command.label().into(),
            purpose: FormPurpose::Local(Box::new(LocalIntent {
                command,
                context,
                plan: None,
                selections: BTreeMap::new(),
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
        let fields = workspace
            .fields(command)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|input| {
                let mut field =
                    Field::text(&input.key, &input.label, &input.value, input.multiline)?;
                field.secret = input.secret;
                field.choices = input.choices;
                Ok(field)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
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
    pub fn compare(intent: crate::comparison::CompareIntent) -> Result<Self, String> {
        let mut mode = Field::text(
            "mode",
            "Question: snapshot or proposed-branch changes",
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
                "A: starting saved version",
                &intent.base.value,
                false,
            )?,
            Field::text("head", "B: target saved version", &intent.head.value, false)?,
            mode,
        ];
        Ok(Self {
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
            FormPurpose::Pr(_) | FormPurpose::PrAction(_) | FormPurpose::PrMetadata(_) => {
                crate::app::View::Inbox
            }
            FormPurpose::Local(_) | FormPurpose::Compare(_) => crate::app::View::Repository,
            FormPurpose::Actions(_) => crate::app::View::Actions,
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
    pub fn invalidate_plan(&mut self) {
        self.metadata_preview = None;
        if matches!(
            self.purpose,
            FormPurpose::Pr(_) | FormPurpose::PrMetadata(_)
        ) && self.uses_prepared_preview()
        {
            self.preview = false;
            self.preview_scroll = 0;
            self.preview_requested = false;
            self.submit_requested = false;
        }
        match &mut self.purpose {
            FormPurpose::Local(intent) => intent.plan = None,
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
    }
    pub fn plan_ready(&self) -> bool {
        match &self.purpose {
            FormPurpose::Local(intent) => intent.plan.is_some(),
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
        if self.metadata_original() {
            return &self.metadata_original_lines;
        }
        if self.actions_publication.is_some() {
            return &self.actions_original_lines;
        }
        match &self.purpose {
            FormPurpose::Local(intent) => intent
                .plan
                .as_ref()
                .map_or(&[], |plan| plan.preview.as_slice()),
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
        let mut actions = vec![
            FormAction::Preview,
            FormAction::Editor,
            FormAction::Submit,
            FormAction::Close,
        ];
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
                format!("local/{path}/operation/{}", intent.command.key())
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
        let values: BTreeMap<&str, &str> = self
            .fields
            .iter()
            .filter(|field| !field.secret)
            .map(|field| (field.key.as_str(), field.editor.text.as_str()))
            .collect();
        if matches!(
            self.purpose,
            FormPurpose::Pr(_) | FormPurpose::PrAction(_) | FormPurpose::PrMetadata(_)
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
    pub fn restore_draft(&mut self, body: &str) -> Result<(), String> {
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
        match action {
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
            FormAction::Editor if matches!(self.purpose, FormPurpose::PrMetadata(_)) => {
                self.error="Metadata values use genuine indexed choices; external JSON/text cannot grant selection or publication authority. Use F4 Choose.".into();
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
            FormAction::Submit | FormAction::Close | FormAction::OpenPicker(_) => {}
        }
    }
    pub fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<FormAction> {
        let control = modifiers.contains(KeyModifiers::CONTROL);
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        if code == KeyCode::Esc {
            return Some(FormAction::Close);
        }
        if self.submitting {
            return None;
        }
        if code == KeyCode::F(2) && self.full_input_available(self.active_field) {
            return Some(FormAction::FullInput(self.active_field));
        }
        if code == KeyCode::F(4) && self.field_has_picker(self.active_field) {
            return Some(FormAction::OpenPicker(self.active_field));
        }
        if code == KeyCode::F(9) && matches!(self.purpose, FormPurpose::Compare(_)) {
            return Some(FormAction::Swap);
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
        if control && code == KeyCode::Enter {
            return Some(FormAction::Submit);
        }
        if self.preview && self.uses_prepared_preview() {
            match code {
                KeyCode::Down => self.preview_scroll += 1,
                KeyCode::Up => self.preview_scroll = self.preview_scroll.saturating_sub(1),
                KeyCode::PageDown => self.preview_scroll += 10,
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(10),
                _ => {}
            }
        }
        if matches!(code, KeyCode::Tab | KeyCode::BackTab) {
            let count = self.fields.len() + self.footer_actions().len();
            self.focus = if code == KeyCode::Tab {
                (self.focus + 1) % count
            } else {
                (self.focus + count - 1) % count
            };
            if self.focus < self.fields.len() {
                self.active_field = self.focus;
            }
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

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let Some(form) = &mut app.form else {
        return;
    };
    form.retire_changed_current_input();
    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(4).min(28);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
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
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", safe_text(&form.title)))
            .style(theme.base()),
        rect,
    );
    let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
    let footer_height =
        if form.error.is_empty() { 3 } else { 6 } + if form.fields.len() > 1 { 1 } else { 0 };
    let mut y = inner.y;
    let context = match &form.purpose {
        FormPurpose::Pr(intent) => format!(
            "@{} on {} • head {}",
            intent.identity.login, intent.identity.host, intent.target.head_sha
        ),
        FormPurpose::PrAction(intent) => format!(
            "@{} on {} • {}#{} • head {}",
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
    frame.render_widget(
        Paragraph::new(safe_text(&context)).style(theme.style(SemanticToken::Muted)),
        Rect::new(inner.x, y, inner.width, 1),
    );
    let identity = match &form.purpose {
        FormPurpose::Pr(intent) => Some(&intent.identity),
        FormPurpose::PrAction(intent) => Some(&intent.context.identity),
        FormPurpose::PrMetadata(intent) => Some(&intent.identity),
        FormPurpose::Actions(intent) => Some(&intent.workspace.identity),
        FormPurpose::Corpus(intent) => Some(&intent.identity),
        FormPurpose::Local(_) | FormPurpose::Compare(_) => None,
    };
    if let Some(identity) = identity {
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
    frame.render_widget(
        Paragraph::new(if matches!(form.purpose, FormPurpose::Compare(_)) {
            "READ-ONLY COMPARISON • No checkout, fetch, index or working-file writes"
        } else if form.is_local() {
            if form.preview {
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
        } else if matches!(
            form.purpose,
            FormPurpose::PrAction(_) | FormPurpose::PrMetadata(_)
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
        } else if form.preview {
            "PREVIEW • Nothing published"
        } else {
            "LOCAL DRAFT • Submit is the only publishing action"
        })
        .style(theme.style(SemanticToken::Pending)),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
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
    form.field_rects = vec![Rect::default(); form.fields.len()];
    if form.uses_prepared_preview() && form.preview {
        let visible = inner.bottom().saturating_sub(y + footer_height) as usize;
        let wrapped = form
            .plan_lines()
            .iter()
            .flat_map(|line| crate::ui::notice_lines(&safe_text(line), inner.width))
            .collect::<Vec<_>>();
        form.preview_scroll = form
            .preview_scroll
            .min(wrapped.len().saturating_sub(visible));
        let lines = form.preview_scroll;
        frame.render_widget(
            Paragraph::new(
                wrapped
                    .into_iter()
                    .skip(lines)
                    .take(visible)
                    .collect::<Vec<_>>(),
            )
            .style(theme.base()),
            Rect::new(
                inner.x,
                y,
                inner.width,
                inner.bottom().saturating_sub(y + footer_height),
            ),
        );
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
    frame.render_widget(
        Paragraph::new(safe_text(&form.error))
            .style(theme.style(SemanticToken::Failure))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((form.error_scroll.min(u16::MAX as usize) as u16, 0)),
        error_area,
    );
    frame.render_widget(
        Paragraph::new(if form.error.is_empty() {
            "Tab: controls • Ctrl-A: select all • Ctrl-P: preview • Esc: keep draft"
        } else {
            "Tab: controls • Esc: keep draft"
        }),
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
    );
    let mut x = inner.x;
    let y = inner.bottom().saturating_sub(1);
    for (offset, (label, action)) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        let width = (label.len() as u16).min(inner.right().saturating_sub(x));
        let area = Rect::new(x, y, width, 1);
        let disabled = (form.submitting && action != FormAction::Close)
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
        for (offset, label, action) in [
            (0, "[F7 Error↑]", FormAction::ErrorScroll(-3)),
            (13, "[F8 Error↓]", FormAction::ErrorScroll(3)),
        ] {
            let area = Rect::new(
                inner.x + 36.min(inner.width) + offset.min(inner.width.saturating_sub(36)),
                inner.bottom().saturating_sub(2),
                12.min(inner.width.saturating_sub(36 + offset)),
                1,
            );
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Failure)),
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
    if form.fields.len() > 1 {
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
    if form.actions_publication.is_some() || form.metadata_original() {
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
