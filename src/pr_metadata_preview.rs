//! Worker-prepared immutable local text-effect preview. This is a snapshot, not
//! authentication, permission, invocation or receipt authority. Publication must
//! still perform fresh account/PR/selected-field guards before a durable claim.
//! No Serialize/Deserialize: drafts restore text, never preview approval.
use crate::{
    editor::grapheme_width,
    forms::{PrFormKind, PrIntent},
    identity::ActorIdentity,
    pr_write_journal::PrWriteTarget,
    providers::{
        gh::{Actor, PrTextEdit},
        runner::sanitize_terminal,
    },
    search::model::valid_sha,
    storage::Scope,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, io};
use unicode_segmentation::UnicodeSegmentation;

const MAX_FIELD_BYTES: usize = 65_536;
const MAX_IDENTITY_BYTES: usize = 4096;
const MAX_RAW_BYTES: usize = 256 * 1024;
const MAX_SERIALIZED_BYTES: usize = 1024 * 1024;
const MAX_PREPARED_BYTES: usize = 1024 * 1024;
const MAX_PREPARED_ROWS: usize = 4096;
const MAX_ROW_BYTES: usize = 4096;
const WRAP_CELLS: usize = 68;

#[derive(Clone)]
pub struct PrMetadataPreview {
    target: PrWriteTarget,
    kind: PrFormKind,
    fingerprint: String,
    preview: Vec<String>,
    edit: PrTextEdit,
    actor: ActorIdentity,
    disabled_reason: Option<String>,
    selected_base: Option<crate::pr_metadata_choices::PrMetadataValue>,
}
impl fmt::Debug for PrMetadataPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrMetadataPreview")
            .field("kind", &self.kind)
            .field("fingerprint", &self.fingerprint)
            .field("prepared_rows", &self.preview.len())
            .field("disabled_reason", &self.disabled_reason)
            .field("payload", &"[PRIVATE TEXT]")
            .finish()
    }
}
impl PrMetadataPreview {
    pub fn supports(kind: PrFormKind) -> bool {
        matches!(
            kind,
            PrFormKind::Title | PrFormKind::Description | PrFormKind::Base
        )
    }
    pub fn prepare(intent: &PrIntent, values: &BTreeMap<String, String>) -> Result<Self, String> {
        Self::prepare_inner(intent, values, None)
    }
    /// Native changed Base values require a genuine original-bound chosen row.
    /// NoChange can still be inspected without reading or selecting any branch.
    pub fn prepare_selected(
        intent: &PrIntent,
        values: &BTreeMap<String, String>,
        selected: Option<&crate::pr_metadata_base::PrBaseSelection>,
    ) -> Result<Self, String> {
        if intent.kind == PrFormKind::Base && values.get("body") != Some(&intent.target.base_ref) {
            let selected = selected.ok_or("Choose a verified Base branch before Preview")?;
            selected.validate(intent, values)?;
            Self::prepare_inner(intent, values, Some(selected))
        } else {
            Self::prepare(intent, values)
        }
    }
    fn prepare_inner(
        intent: &PrIntent,
        values: &BTreeMap<String, String>,
        selected: Option<&crate::pr_metadata_base::PrBaseSelection>,
    ) -> Result<Self, String> {
        let input = checked_input(intent, values)?;
        let fingerprint = fingerprint(intent, &input)?;
        let actor = ActorIdentity::github(
            &intent.identity.host,
            &Actor {
                login: intent.identity.login.clone(),
                ..Actor::default()
            },
        );
        let author = ActorIdentity::github(&intent.identity.host, &intent.target.pr.author);
        let no_change = input.current == input.requested;
        let disabled_reason=no_change.then(||format!("No change • requested {} exactly matches the loaded snapshot; confirmation is unavailable",input.label));
        let mut rows = PreparedRows::default();
        rows.text(&format!("PR metadata preview • {}", input.label))?;
        if let Some(reason) = &disabled_reason {
            rows.text(reason)?;
        }
        rows.text(&format!(
            "Target: {}#{}",
            input.target.repository(),
            input.target.number()
        ))?;
        rows.text(&format!(
            "Caller: {} • account #{} • {}",
            actor.label(),
            intent.identity.account_id.expect("positive scope checked"),
            input.target.scope().host
        ))?;
        rows.text(&format!(
            "PR author: {} • numeric author identity: Source unknown in this DTO",
            author.label()
        ))?;
        rows.text(&format!("Original PR node: {}", input.target.node_id()))?;
        rows.text(&format!("Observed head: {}", intent.target.head_sha))?;
        rows.text(&format!("Observed base: {}", intent.target.base_sha))?;
        rows.text("Snapshot only • no gh/Git request; current account, comparison and selected field are rechecked before publication.")?;
        rows.text(match intent.kind {
            PrFormKind::Title=>"Effect: replace only this PR title. No description/base/ownership/reviewer fields are sent.",
            PrFormKind::Description=>"Effect: replace only this PR description. Closing keywords are text; issue linking/resolution is not confirmed by this snapshot.",
            PrFormKind::Base=>"Effect: change only the destination base branch.",
            _=>unreachable!("supported field checked"),
        })?;
        if intent.kind == PrFormKind::Base {
            rows.text(
                "Local files, checkout and the PR head branch are not changed by this request.",
            )?;
            rows.text("Changing the base can remove commits from the PR timeline and make review comments outdated; inspect the resulting comparison after server read-back.")?;
        }
        rows.text(&format!(
            "Current {} • {} UTF-8 bytes:",
            input.label,
            input.current.len()
        ))?;
        rows.value(input.current)?;
        rows.text(&format!(
            "Requested {} • {} UTF-8 bytes:",
            input.label,
            input.requested.len()
        ))?;
        rows.value(input.requested)?;
        if intent.kind == PrFormKind::Base {
            if let Some(selected) = selected {
                rows.text(&format!("Requested base tip: {}", selected.tip()?))?;
                rows.text("Observed eligible branch snapshot • exact raw name and full tip are freshly rechecked before the durable publication attempt.")?;
                let raw = selected.name()?;
                if sanitize_terminal(raw) != raw {
                    rows.text("Requested exact raw branch (escaped; safe display differs):")?;
                    rows.text(
                        &raw.chars()
                            .flat_map(char::escape_default)
                            .collect::<String>(),
                    )?;
                }
            } else {
                rows.text("Requested base commit: Source unknown • this text snapshot does not fetch a branch tip; resulting comparison needs server read-back.")?;
            }
        }
        rows.text("Other metadata values/permissions: Source unknown in this text snapshot. Labels, assignees, reviewers and milestone are not requested.")?;
        rows.text("Only explicit Confirm publishes this exact prepared intent; editing, closing, external editing or restoring a draft requires a new Preview.")?;
        let edit = PrTextEdit {
            title: (intent.kind == PrFormKind::Title).then(|| input.requested.to_owned()),
            body: (intent.kind == PrFormKind::Description).then(|| input.requested.to_owned()),
            base: (intent.kind == PrFormKind::Base).then(|| input.requested.to_owned()),
        };
        Ok(Self {
            target: input.target,
            kind: intent.kind,
            fingerprint,
            preview: rows.rows,
            edit,
            actor,
            disabled_reason,
            selected_base: selected.map(|selected| selected.value().clone()),
        })
    }
    pub fn validate(
        &self,
        intent: &PrIntent,
        values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let input = checked_input(intent, values)?;
        if self.kind != intent.kind || self.fingerprint != fingerprint(intent, &input)? {
            return Err("Metadata input/original target changed after Preview; inspect a new exact-effect Preview before confirmation".into());
        }
        if let Some(reason) = &self.disabled_reason {
            return Err(reason.clone());
        }
        Ok(())
    }
    pub fn preview(&self) -> &[String] {
        &self.preview
    }
    pub fn validate_selected(
        &self,
        intent: &PrIntent,
        values: &BTreeMap<String, String>,
        selected: Option<&crate::pr_metadata_base::PrBaseSelection>,
    ) -> Result<(), String> {
        self.validate(intent, values)?;
        if intent.kind == PrFormKind::Base {
            let selected = selected.ok_or("Choose a verified Base branch before Preview")?;
            selected.validate(intent, values)?;
            if self.selected_base.as_ref() != Some(selected.value()) {
                return Err(
                    "Chosen Base tip changed after Preview; choose and preview again".into(),
                );
            }
        }
        Ok(())
    }
    pub fn edit(&self) -> PrTextEdit {
        self.edit.clone()
    }
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub fn scope(&self) -> &Scope {
        self.target.scope()
    }
    pub fn actor(&self) -> &ActorIdentity {
        &self.actor
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }
}

struct CheckedInput<'a> {
    target: PrWriteTarget,
    current: &'a str,
    requested: &'a str,
    label: &'static str,
}
fn checked_input<'a>(
    intent: &'a PrIntent,
    values: &'a BTreeMap<String, String>,
) -> Result<CheckedInput<'a>, String> {
    let (current,label)=match intent.kind {
        PrFormKind::Title=>(intent.target.pr.title.as_str(),"Title"),
        PrFormKind::Description=>(intent.target.pr.body.as_str(),"Description"),
        PrFormKind::Base=>(intent.target.base_ref.as_str(),"Base branch"),
        _=>return Err("Exact metadata preview supports only Title, Description and Base branch; other legacy forms are unchanged".into()),
    };
    if values.len() != 1 || !values.contains_key("body") {
        return Err("Metadata preview requires exactly the selected body field; no extra/missing fields or arbitrary JSON".into());
    }
    let requested = values["body"].as_str();
    if current.len() > MAX_FIELD_BYTES || requested.len() > MAX_FIELD_BYTES {
        return Err("Metadata text exceeds 65536 UTF-8 bytes before preview preparation".into());
    }
    if intent.kind != PrFormKind::Description && requested.trim().is_empty() {
        return Err("The selected metadata field needs nonempty text".into());
    }
    if intent.kind == PrFormKind::Base && requested.chars().any(char::is_control) {
        return Err(
            "Base branch contains a control character; no exact branch effect is prepared".into(),
        );
    }
    if !valid_sha(&intent.target.head_sha) || !valid_sha(&intent.target.base_sha) {
        return Err(
            "Metadata preview needs the original full 40/64 hexadecimal head and base identities"
                .into(),
        );
    }
    let mut bytes = current.len() + requested.len();
    for value in [
        &intent.identity.host,
        &intent.identity.login,
        &intent.target.pr.host,
        &intent.target.pr.id,
        &intent.target.pr.repository,
        &intent.target.head_sha,
        &intent.target.base_sha,
        &intent.target.head_ref,
        &intent.target.base_ref,
        &intent.target.pr.state,
        &intent.target.pr.author.login,
        &intent.target.pr.author.actor_type,
    ] {
        if value.len() > MAX_IDENTITY_BYTES {
            return Err(
                "Raw metadata identity byte budget exceeded before cloning/formatting".into(),
            );
        }
        bytes = bytes.saturating_add(value.len());
    }
    for value in [
        &intent.target.pr.author.display_name,
        &intent.target.pr.author.avatar_url,
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > MAX_IDENTITY_BYTES {
            return Err("Raw metadata actor byte budget exceeded before cloning/formatting".into());
        }
        bytes = bytes.saturating_add(value.len());
    }
    if bytes > MAX_RAW_BYTES {
        return Err("Raw metadata aggregate byte budget exceeded before cloning/formatting".into());
    }
    if current
        .split('\n')
        .count()
        .saturating_add(requested.split('\n').count())
        > MAX_PREPARED_ROWS - 32
    {
        return Err(
            "Metadata preview row budget exceeded before formatting; no silent truncation".into(),
        );
    }
    if [current, requested].iter().any(|value| {
        value
            .graphemes(true)
            .any(|grapheme| grapheme.len() > MAX_ROW_BYTES)
    }) {
        return Err("Metadata preview grapheme byte budget exceeded before formatting".into());
    }
    let target =
        PrWriteTarget::new(&intent.identity, &intent.target).map_err(|error| error.to_string())?;
    let target = PrWriteTarget::for_source(
        target.scope(),
        &intent.target.pr.repository.to_ascii_lowercase(),
        target.node_id(),
        target.number(),
    )
    .map_err(|error| error.to_string())?;
    Ok(CheckedInput {
        target,
        current,
        requested,
        label,
    })
}
fn fingerprint(intent: &PrIntent, input: &CheckedInput<'_>) -> Result<String, String> {
    let mut writer = BoundedFingerprint {
        hasher: Sha256::new(),
        bytes: 0,
    };
    // Serialize references directly into a counting hash, never a cloned body
    // or a JSON preview. Unselected field text is deliberately omitted.
    serde_json::to_writer(
        &mut writer,
        &(
            input.target.scope(),
            input.target.node_id(),
            input.target.repository(),
            input.target.number(),
            intent.kind.key(),
            &intent.identity.login,
            &intent.target.head_sha,
            &intent.target.base_sha,
            &intent.target.head_ref,
            &intent.target.base_ref,
            &intent.target.pr.state,
            intent.target.pr.draft,
            intent.target.merged,
            &intent.target.pr.author,
            input.current,
            input.requested,
        ),
    )
    .map_err(|_| "Metadata serialized fingerprint exceeds its 1 MiB input budget".to_owned())?;
    Ok(format!("{:x}", writer.hasher.finalize()))
}
struct BoundedFingerprint {
    hasher: Sha256,
    bytes: usize,
}
impl io::Write for BoundedFingerprint {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        if value.len() > MAX_SERIALIZED_BYTES.saturating_sub(self.bytes) {
            return Err(io::Error::other("bounded metadata fingerprint"));
        }
        self.bytes += value.len();
        self.hasher.update(value);
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Default)]
struct PreparedRows {
    rows: Vec<String>,
    bytes: usize,
}
impl PreparedRows {
    fn push(&mut self, row: String) -> Result<(), String> {
        if self.rows.len() >= MAX_PREPARED_ROWS
            || row.len() > MAX_ROW_BYTES
            || row.len() > MAX_PREPARED_BYTES.saturating_sub(self.bytes)
        {
            return Err(
                "Prepared metadata preview row/byte budget exceeded; no silent truncation".into(),
            );
        }
        self.bytes += row.len();
        self.rows.push(row);
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), String> {
        let safe = sanitize_terminal(text).replace('\t', "⇥");
        for line in safe.split('\n') {
            let mut current = String::new();
            let mut cells = 0usize;
            for word in line.split_inclusive(char::is_whitespace) {
                let width = grapheme_width(word);
                if !current.is_empty() && cells.saturating_add(width) > WRAP_CELLS {
                    self.push(std::mem::take(&mut current))?;
                    cells = 0;
                }
                for grapheme in word.graphemes(true) {
                    let width = grapheme_width(grapheme);
                    if !current.is_empty() && cells.saturating_add(width) > WRAP_CELLS {
                        self.push(std::mem::take(&mut current))?;
                        cells = 0;
                    }
                    if grapheme.len() > MAX_ROW_BYTES.saturating_sub(current.len()) {
                        return Err("Prepared metadata grapheme row byte budget exceeded".into());
                    }
                    current.push_str(grapheme);
                    cells = cells.saturating_add(width);
                }
            }
            self.push(current)?;
        }
        Ok(())
    }
    fn value(&mut self, raw: &str) -> Result<(), String> {
        if raw.is_empty() {
            return self.text("(empty in the loaded/requested text snapshot)");
        }
        let safe = sanitize_terminal(raw);
        if safe != raw {
            self.text("Safe display omits control/directional decorations; exact raw request bytes are preserved, not rewritten.")?;
        }
        self.text(raw)
    }
}

#[cfg(test)]
#[path = "pr_metadata_preview_tests.rs"]
mod tests;
