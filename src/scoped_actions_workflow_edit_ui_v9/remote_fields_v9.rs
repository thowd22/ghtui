//! Worker-prepared data only. A hidden seed cannot be a head member, remote
//! association, ref observation, permission or publication approval.
use super::*;

pub(super) const REMOTE_KEYS: [&str; 3] = ["head_member", "configured_remote", "base_branch"];
const REMOTE_LABELS: [&str; 3] = [
    "Catalogue head repository (Preview rechecks permission/network)",
    "Exact configured push remote name (no inferred origin)",
    "Exact base branch name (fresh actual ref checked at Preview)",
];
#[derive(Clone)]
pub(super) struct RemoteFieldsSeedV9 {
    pub(super) fields: Vec<Field>,
}
impl RemoteFieldsSeedV9 {
    pub(super) fn owned_bytes(&self) -> Result<usize, String> {
        fields_bytes(&self.fields, self.fields.capacity())
    }
}
pub(crate) fn preparation_bound(intent: &WorkflowEditIntentV9) -> Result<usize, String> {
    let rows = intent.source().original().source().repositories();
    rows.iter()
        .try_fold(8192usize + 3 * std::mem::size_of::<Field>(), |n, row| {
            // Values use only original indexes. Labels are bounded original names
            // plus positive numeric display IDs, not copied repository DTOs.
            if row.full_name.len() > 256 || row.full_name.chars().any(char::is_control) {
                return Err(refused());
            }
            row.full_name
                .len()
                .checked_add(96)
                .and_then(|bytes| bytes.checked_mul(4))
                .and_then(|bytes| bytes.checked_add(4 * std::mem::size_of::<String>()))
                .and_then(|bytes| bytes.checked_add(n))
                .filter(|n| *n <= 8 * 1024 * 1024)
                .ok_or_else(refused)
        })
}
pub(super) fn prepare(intent: &WorkflowEditIntentV9) -> Result<RemoteFieldsSeedV9, String> {
    let bound = preparation_bound(intent)?; // Borrowed preflight BEFORE copies.
    let mut fields = Vec::with_capacity(3);
    for index in 0..3 {
        let mut field = Field::text(REMOTE_KEYS[index], REMOTE_LABELS[index], "", false)?;
        field.debug_redacted = true;
        if index == 0 {
            let rows = intent.source().original().source().repositories();
            field.choices = Vec::with_capacity(rows.len());
            field.choice_labels = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                field.choices.push(index.to_string());
                field
                    .choice_labels
                    .push(format!("{} • repository ID {}", row.full_name, row.id));
            }
        }
        fields.push(field);
    }
    let seed = RemoteFieldsSeedV9 { fields };
    if seed.owned_bytes()? > bound {
        return Err(refused());
    }
    Ok(seed)
}
fn fields_bytes(fields: &[Field], capacity: usize) -> Result<usize, String> {
    fields
        .iter()
        .try_fold(capacity * std::mem::size_of::<Field>(), |n, field| {
            let choices = field
                .choices
                .iter()
                .chain(&field.choice_labels)
                .try_fold(0usize, |n, value| n.checked_add(value.capacity()))
                .ok_or_else(refused)?;
            n.checked_add(field.key.capacity())
                .and_then(|n| n.checked_add(field.label.capacity()))
                .and_then(|n| n.checked_add(field.editor.text.capacity()))
                .and_then(|n| {
                    n.checked_add(
                        (field.choices.capacity() + field.choice_labels.capacity())
                            * std::mem::size_of::<String>(),
                    )
                })
                .and_then(|n| n.checked_add(choices))
                .ok_or_else(refused)
        })
}
pub(super) fn valid_rows(form: &Form) -> bool {
    form.fields.len() == 9
        && form.fields[6..]
            .iter()
            .zip(REMOTE_KEYS)
            .all(|(field, key)| {
                field.key == key
                    && !field.secret
                    && field.editor.text.len() <= 256
                    && !field.editor.text.contains('\0')
            })
}
pub(super) fn selection(
    form: &Form,
) -> Result<Arc<crate::scoped_actions_workflow_edit_v9::WorkflowRemoteSelectionV9>, String> {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return Err(refused());
    };
    if !valid_rows(form) {
        return Err(refused());
    }
    let value = form.fields[6].editor.text.as_str();
    if value.is_empty()
        || value.len() > 20
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(refused());
    }
    let index = value.parse::<usize>().map_err(|_| refused())?;
    if form.fields[6]
        .choices
        .get(index)
        .is_none_or(|original| original != value)
    {
        return Err(refused());
    }
    crate::scoped_actions_workflow_edit_v9::WorkflowRemoteSelectionV9::capture(
        state.committed.as_ref().ok_or_else(refused)?,
        index,
        &form.fields[7].editor.text,
        &form.fields[8].editor.text,
    )
}
pub(super) fn matches_selection(
    form: &Form,
    selected: &crate::scoped_actions_workflow_edit_v9::WorkflowRemoteSelectionV9,
) -> bool {
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return false;
    };
    valid_rows(form)
        && state
            .committed
            .as_ref()
            .is_some_and(|committed| Arc::ptr_eq(committed, selected.committed()))
        && selected.member().is_original(state.intent())
        && form.fields[6].editor.text.parse::<usize>().ok() == Some(selected.member().index())
        && form.fields[6]
            .choices
            .get(selected.member().index())
            .is_some_and(|value| *value == form.fields[6].editor.text)
        && form.fields[7].editor.text == selected.remote_name()
        && form.fields[8].editor.text == selected.base_branch()
}
