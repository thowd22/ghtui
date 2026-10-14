//! Worker-side prepared repository management. No automatic clone or retry.
use crate::{
    corpus_ui,
    drafts::DraftClient,
    forms::{Form, FormPurpose},
    providers::{gh::GithubProvider, runner::CancellationToken},
    runtime::PreparedView,
    search::workspace::SearchWorkspace,
    storage::Cache,
};
use std::path::{Path, PathBuf};

pub fn open(
    provider: &GithubProvider,
    base: PathBuf,
    repositories: Vec<String>,
    mut search: SearchWorkspace,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let cache = Cache::open(database).map_err(|error| error.to_string())?;
    let intent = corpus_ui::load(
        provider,
        base,
        repositories,
        Some(search.scope.clone()),
        search.selections.clone(),
        &cache,
        cancel,
    )?;
    let identity = intent.identity.clone();
    corpus_ui::apply_to_search(&intent, &mut search, &cache)?;
    let mut form = Form::corpus(intent)?;
    if let Some(draft) = cache
        .draft(&form.scope(), &form.resource())
        .map_err(|error| error.to_string())?
    {
        form.restore_draft(&draft)?;
    }
    Ok(PreparedView {
        status:
            "Repository management ready • existing selections restored; nothing cloned or fetched"
                .into(),
        context: Some(search.context()),
        search: Some(search),
        form: Some(form),
        verified_identity: Some(identity),
        restore_search_input: true,
        ..Default::default()
    })
}

pub fn submit(
    mut form: Form,
    mut search: SearchWorkspace,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let values = form.values();
    let cache = Cache::open(database).map_err(|error| error.to_string())?;
    let FormPurpose::Corpus(intent) = &mut form.purpose else {
        return Err("Not a corpus-management form".into());
    };
    let result = corpus_ui::submit(intent, &values, &cache, provider, true, cancel);
    let identity = intent.identity.clone();
    form.submitting = false;
    let mut status;
    match result {
        Ok(outcome) => {
            status = format!("Repository operation completed • {}", outcome.message);
            if let Err(error) = corpus_ui::apply_to_search(intent, &mut search, &cache) {
                search = SearchWorkspace::new(intent.scope()?, intent.current_selections.clone());
                status.push_str(&format!(
                    " • selection refresh failed: {error}; do not repeat the operation"
                ));
            }
            form.submitted = true;
            form.dirty = false;
            if let Err(error) = drafts.remove(form.scope(), form.resource()) {
                status.push_str(&format!(" • draft cleanup failed: {error}"));
            }
        }
        Err(error) => {
            if crate::pr_ui::account_failure(&error)
                || error.contains("does not match") && error.contains("account")
            {
                return Ok(PreparedView {clear_private:true,status:format!("Repository account changed • private selections hidden; inspect stored result before retrying: {error}"),..Default::default()});
            }
            // Cancellation or readback failure can follow a filesystem effect.
            // Preserve the intent and require a new preview; never rerun here.
            status = format!("Repository operation needs attention • {error}; inspect mapping/recovery before another attempt");
            form.error = status.clone();
            form.invalidate_plan();
        }
    }
    Ok(PreparedView {
        context: Some(search.context()),
        status,
        search: Some(search),
        form: Some(form),
        verified_identity: Some(identity),
        restore_search_input: true,
        ..Default::default()
    })
}
