//! Durable adapter for the original seven PR forms. Only explicit publication
//! reserves a journal permit; closing, editing and previewing never call this.
use crate::{
    forms::{PrFormKind, PrIntent},
    github_workspace::GithubWorkspace,
    pr_actions::{PrActionContext, PrActionEffect, PrCommand, PrExecution, PrPlan},
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
        PrWriteMarker, PrWriteTarget,
    },
    providers::{
        gh::{GithubProvider, PrTextEdit, ReviewEvent},
        runner::{sanitize_terminal, CancellationToken},
    },
};
use std::collections::BTreeMap;

#[allow(clippy::too_many_arguments)]
pub fn execute_legacy<F>(
    intent: &PrIntent,
    workspace: &GithubWorkspace,
    values: &BTreeMap<String, String>,
    provider: &GithubProvider,
    journal: &mut PrWriteJournal,
    confirmed: bool,
    restored: Option<&PrPublicationReference>,
    persist_publication: F,
    cancel: &CancellationToken,
) -> PrExecution
where
    F: FnOnce(&PrWriteMarker) -> Result<(), String>,
{
    let refuse = |error: String| PrExecution::Refused {
        error: sanitize_terminal(&error),
    };
    if !confirmed {
        return refuse("Confirmation required; no GitHub write was sent.".into());
    }
    if cancel.is_cancelled() {
        return refuse("Publication cancelled before invocation.".into());
    }
    if values.keys().any(|key| key != "body") {
        return refuse("Unexpected legacy PR form field; no GitHub request was sent.".into());
    }
    let body = values.get("body").cloned().unwrap_or_default();
    if body.len() > 65_536
        || (!matches!(intent.kind, PrFormKind::Approve | PrFormKind::Description)
            && body.trim().is_empty())
    {
        return refuse("The selected PR field needs valid text within 65536 UTF-8 bytes.".into());
    }
    if intent.kind == PrFormKind::Base && body.chars().any(char::is_control) {
        return refuse(
            "Base branch contains a control character; no GitHub request was sent.".into(),
        );
    }
    let Some(page) = &workspace.pr else {
        return refuse("The exact PR is no longer active.".into());
    };
    let active = &page.details.summary;
    if !workspace.identity.same_account(&intent.identity)
        || active.pr.id != intent.target.pr.id
        || active.pr.host != intent.target.pr.host
        || active.pr.number != intent.target.pr.number
        || !active
            .pr
            .repository
            .eq_ignore_ascii_case(&intent.target.pr.repository)
    {
        return refuse(
            "Legacy form belongs to a different active PR/account; no GitHub request was sent."
                .into(),
        );
    }
    let target = match PrWriteTarget::new(&intent.identity, &intent.target) {
        Ok(target) => target,
        Err(error) => return refuse(error.to_string()),
    };
    let publication_intent = PrWriteIntent {
        command: if intent.kind == PrFormKind::Comment {
            "new-comment".into()
        } else {
            format!("legacy-{}", intent.kind.key())
        },
        selected_resource: "pr".into(),
        observed_head: intent.target.head_sha.clone(),
        values: BTreeMap::from([("body".into(), body.clone())]),
    };
    if let Some(reference) = restored {
        match journal.confirmed_original(&target, reference, &publication_intent) {
            Ok(Some(marker)) => {
                if let Err(error) = crate::pr_projection::invalidate_confirmed_discussions(
                    journal.database_path(),
                    &intent.identity,
                    &intent.target,
                    reference,
                ) {
                    return PrExecution::ConfirmedButRefreshFailed {
                        effect: PrActionEffect::LegacyConfirmed(marker.confirmed.clone().expect("confirmed_original validates its genuine receipt")),
                        publication: marker.publication_reference(),
                        error: sanitize_terminal(&format!("Original publication remains confirmed, but discussion-cache visibility failed: {error}. Private search stays hidden; do not resend.")),
                        private_visible: false,
                    };
                }
                return PrExecution::RestoredAlreadyConfirmed { marker };
            }
            Ok(None) => {}
            Err(error) => return refuse(error.to_string()),
        }
    }
    match journal.blocking(&target) {
        Ok(Some(_))=>return refuse("This PR has an unresolved publication. Inspect and explicitly acknowledge its outcome before another write.".into()),Ok(None)=>{},Err(error)=>return refuse(error.to_string()),
    }
    if intent.kind == PrFormKind::Comment {
        let mut context = match PrActionContext::from_workspace(workspace, None) {
            Ok(context) => context,
            Err(error) => return refuse(error),
        };
        context.identity = intent.identity.clone();
        context.target = intent.target.clone();
        let plan = match PrPlan::prepare(
            provider,
            &context,
            PrCommand::NewComment,
            &publication_intent.values,
            cancel,
        ) {
            Ok(plan) => plan,
            Err(error) => return refuse(error),
        };
        return plan.execute(
            provider,
            journal,
            true,
            restored,
            persist_publication,
            cancel,
        );
    }
    if matches!(
        intent.kind,
        PrFormKind::Approve | PrFormKind::RequestChanges
    ) && intent
        .target
        .pr
        .author
        .login
        .eq_ignore_ascii_case(&intent.identity.login)
    {
        return refuse("You cannot approve or request changes on your own PR.".into());
    }
    let current = match provider.identity(cancel) {
        Ok(current) => current,
        Err(error) => return refuse(error.to_string()),
    };
    if !current.same_account(&intent.identity) {
        return refuse("Account changed before publication; no GitHub write was sent.".into());
    }
    let fresh = match provider.pr_summary(target.repository(), target.number(), cancel) {
        Ok(fresh) => fresh,
        Err(error) => return refuse(error.to_string()),
    };
    if fresh.pr.id != intent.target.pr.id
        || fresh.head_sha != intent.target.head_sha
        || fresh.base_sha != intent.target.base_sha
        || (intent.kind == PrFormKind::Title && fresh.pr.title != intent.target.pr.title)
        || (intent.kind == PrFormKind::Description && fresh.pr.body != intent.target.pr.body)
        || (intent.kind == PrFormKind::Base && fresh.base_ref != intent.target.base_ref)
    {
        return refuse("PR identity, head, base or selected field changed since preview; inspect it before publishing.".into());
    }
    let permit = match journal.begin(&target, &publication_intent) {
        Ok(permit) => permit,
        Err(error) => return refuse(error.to_string()),
    };
    let publication = permit.marker().publication_reference();
    if let Err(error) = persist_publication(permit.marker()) {
        return refuse(format!("Publication metadata could not be saved. No GitHub write was sent; pending marker remains blocked. {error}"));
    }
    if let Err(error) = journal.claim(&permit, &target, &publication_intent) {
        return refuse(error.to_string());
    }
    let receipt = match intent.kind {
        PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment => {
            let event = match intent.kind {
                PrFormKind::Approve => ReviewEvent::Approve,
                PrFormKind::RequestChanges => ReviewEvent::RequestChanges,
                _ => ReviewEvent::Comment,
            };
            provider
                .create_review(&intent.identity, &intent.target, event, &body, cancel)
                .map(|value| PrConfirmedEffect {
                    kind: PrEffectKind::Review,
                    resource_id: value["id"].as_u64().unwrap_or(0).to_string(),
                    node_id: value["node_id"]
                        .as_str()
                        .filter(|node| !node.is_empty())
                        .map(str::to_owned),
                })
        }
        PrFormKind::Title | PrFormKind::Description | PrFormKind::Base => {
            let edit = PrTextEdit {
                title: (intent.kind == PrFormKind::Title).then(|| body.clone()),
                body: (intent.kind == PrFormKind::Description).then(|| body.clone()),
                base: (intent.kind == PrFormKind::Base).then(|| body.clone()),
            };
            provider
                .edit_pr_text(&intent.identity, &intent.target, &edit, cancel)
                .map(|fresh| PrConfirmedEffect {
                    kind: PrEffectKind::Metadata,
                    resource_id: fresh.pr.id.clone(),
                    node_id: Some(fresh.pr.id),
                })
        }
        PrFormKind::Comment => unreachable!("comment uses the journal-only native plan"),
    };
    let receipt = match receipt {
        Ok(receipt) if receipt.resource_id != "0" => receipt,
        Ok(_) => {
            let _ = journal.mark_uncertain(&permit);
            let visibility = crate::pr_projection::unresolved_visibility_diagnostic(
                journal,
                &intent.identity,
                &intent.target,
                &publication,
            );
            return PrExecution::UncertainWrite {
                publication,
                error: format!("Review response has no canonical positive ID; inspect before retrying.{visibility}"),
            };
        }
        Err(error) => {
            let saved = journal.mark_uncertain(&permit);
            let visibility = crate::pr_projection::unresolved_visibility_diagnostic(
                journal,
                &intent.identity,
                &intent.target,
                &publication,
            );
            return PrExecution::UncertainWrite{publication,error:sanitize_terminal(&format!("{error}\nInspect this exact PR before retrying; ordinary refresh is not an acknowledgement.{}{visibility}",saved.err().map_or(String::new(),|error|format!("\nPending marker retained because uncertainty persistence failed: {error}"))))};
        }
    };
    if let Err(error) = journal.mark_confirmed(&permit, &receipt) {
        let visibility = crate::pr_projection::unresolved_visibility_diagnostic(
            journal,
            &intent.identity,
            &intent.target,
            &publication,
        );
        return PrExecution::ConfirmedButJournalFailed{receipt,publication,error:sanitize_terminal(&format!("GitHub confirmed the effect but local receipt persistence failed. Do not publish again; inspect the pending marker. {error}{visibility}"))};
    }
    let effect = PrActionEffect::LegacyConfirmed(receipt);
    if let Err(error) = crate::pr_projection::invalidate_confirmed_discussions(
        journal.database_path(),
        &intent.identity,
        &intent.target,
        &publication,
    ) {
        return PrExecution::ConfirmedButRefreshFailed {
            effect, publication,
            error: sanitize_terminal(&format!("Publication is confirmed, but discussion-cache visibility failed before refresh: {error}. Private search stays hidden; do not resend.")),
            private_visible: false,
        };
    }
    let observation = match provider.inspect_pr_target(&intent.identity, &intent.target, cancel) {
        Ok(observation) => observation,
        Err(error) => {
            return PrExecution::ConfirmedButRefreshFailed {
                effect,
                publication,
                error: sanitize_terminal(&format!(
                "Publication confirmed; private PR refresh failed. Do not publish again. {error}"
            )),
                private_visible: false,
            }
        }
    };
    let updated = match workspace.clone().replace_pr_details_with_current_context(
        observation.details().clone(),
        provider,
        cancel,
    ) {
        Ok(updated) => updated,
        Err(error) => {
            return PrExecution::ConfirmedButRefreshFailed {
                effect,
                publication,
                error: sanitize_terminal(&format!(
                    "Publication confirmed; fresh PR context unavailable. Do not publish again. {error}"
                )),
                private_visible: false,
            };
        }
    };
    PrExecution::Confirmed {
        effect,
        workspace: Box::new(updated),
        publication,
    }
}
