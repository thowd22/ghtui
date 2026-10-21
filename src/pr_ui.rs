//! Worker-side native PR collaboration routing. Rendering never performs I/O.
use crate::{
    app::{PrUiAction, SharedPrInspection},
    drafts::DraftClient,
    forms::{Form, FormPurpose},
    github_workspace::GithubWorkspace,
    pr_actions::{PrActionContext, PrCommand, PrExecution},
    pr_write_journal::{PrWriteJournal, PrWriteTarget},
    providers::{gh::GithubProvider, runner::CancellationToken},
    runtime::PreparedView,
    storage::Cache,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
pub struct PrReactionDisplayRow {
    pub line: ratatui::text::Line<'static>,
    pub actor: Option<crate::identity::ActorIdentity>,
}

/// Prepare the bounded display on a worker, not on every terminal frame.
/// The reaction overlay uses a fixed 68-cell inner width on supported 80+ layouts.
pub fn reaction_display(page: &crate::pr_actions::PrReactionPage) -> Vec<PrReactionDisplayRow> {
    use crate::{app::safe_text, identity::ActorIdentity, ui::notice_lines};
    let mut rows = Vec::new();
    for line in [
        format!(
            "Exact PR: {}#{} • {}",
            page.target.pr.repository, page.target.pr.number, page.target.pr.id
        ),
        format!("Observed head: {}", page.target.head_sha),
        format!(
            "Exact {:?} comment: {} • {}",
            page.comment.kind, page.comment.observed.id, page.comment.observed.node_id
        ),
        format!(
            "Loaded {} reactions • {} • fetched {}",
            page.reactions.len(),
            if page.complete {
                "all fetched pages"
            } else {
                "incomplete catalogue"
            },
            page.fetched_at
        ),
    ] {
        rows.extend(
            notice_lines(&safe_text(&line), 68)
                .into_iter()
                .map(|line| PrReactionDisplayRow { line, actor: None }),
        );
    }
    if page.changed_comment {
        rows.extend(notice_lines("Comment changed since selection • reactions are freshly read; editing still needs Refresh.", 68).into_iter().map(|line| PrReactionDisplayRow {line, actor:None}));
    }
    if page.reactions.is_empty() {
        rows.push(PrReactionDisplayRow {
            line: ratatui::text::Line::from("No reactions on this exact comment."),
            actor: None,
        });
    }
    for reaction in &page.reactions {
        let identity = ActorIdentity::github(&page.identity.host, &reaction.actor);
        let ownership = match reaction.account_id {
            Some(id) if Some(id) == page.identity.account_id => format!("account #{id} • yours"),
            Some(id) => format!("account #{id} • another actor"),
            None => "numeric account unavailable • ownership unverified".into(),
        };
        let label = format!(
            "{} • {} • reaction #{} • {}",
            identity.label(),
            safe_text(&reaction.content),
            reaction.id,
            ownership
        );
        for (index, line) in notice_lines(&label, 64).into_iter().enumerate() {
            rows.push(PrReactionDisplayRow {
                line,
                actor: (index == 0).then(|| identity.clone()),
            });
        }
        let created = if reaction.created_at.is_empty() {
            "unavailable"
        } else {
            &reaction.created_at
        };
        rows.extend(
            notice_lines(
                &safe_text(&format!(
                    "Created: {created} • node {}",
                    if reaction.node_id.is_empty() {
                        "unavailable"
                    } else {
                        &reaction.node_id
                    }
                )),
                68,
            )
            .into_iter()
            .map(|line| PrReactionDisplayRow { line, actor: None }),
        );
    }
    rows
}

/// Recognize explicit account-change diagnostics separately from PR-node
/// staleness; the former must hide the old account's private form.
pub fn account_failure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("account changed")
        || error.contains("github identity changed")
        || error.contains("account identity")
}

pub fn pending_message(database: &Path, workspace: &GithubWorkspace) -> Result<String, String> {
    let Some(page) = &workspace.pr else {
        return Ok(String::new());
    };
    let target = PrWriteTarget::new(&workspace.identity, &page.details.summary)
        .map_err(|e| e.to_string())?;
    let journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
    Ok(journal.blocking(&target).map_err(|e|e.to_string())?.map_or(String::new(), |_| "Unresolved PR publication • inspect the exact result, then explicitly acknowledge it before another write.".into()))
}

pub fn load(
    action: PrUiAction,
    mut context: PrActionContext,
    inspection: Option<SharedPrInspection>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    match action {
        PrUiAction::StateV68(_) | PrUiAction::StateHistoryV74(_) => Err(
            "Original State actions require their closed actual source queue; no legacy fallback"
                .into(),
        ),
        PrUiAction::MetadataTextV25(_)
        | PrUiAction::MetadataMembersV48(_)
        | PrUiAction::MetadataMembersHistoryV48
        | PrUiAction::MetadataHistoryV25(_)
        | PrUiAction::MetadataBaseV32
        | PrUiAction::MetadataBaseHistoryV32
        | PrUiAction::IssueLinksV87(_)
        | PrUiAction::IssueLinkHistoryV87(_) => Err(
            "Original metadata must use the closed current source queue; no legacy fallback".into(),
        ),
        PrUiAction::Metadata(command) => {
            crate::pr_metadata_native::open(command, context, provider, database, cancel)
        }
        PrUiAction::Command(command) => {
            let pending = pending_message(database, &context.workspace)?;
            if !pending.is_empty() {
                return Err(pending);
            }
            if matches!(command, PrCommand::MarkViewed | PrCommand::MarkUnviewed) {
                let selected = context
                    .selected_file
                    .clone()
                    .ok_or("Select an exact file in Review progress first")?;
                let progress=match crate::pr_progress::read(provider,&context.identity,&context.target,cancel) {
                    Ok(progress)=>progress,
                    Err(error) if account_failure(&error)=>return Ok(PreparedView {clear_private:true,status:"Account changed or unavailable • private review progress hidden; nothing sent".into(),..Default::default()}),
                    Err(error)=>return Err(error),
                };
                if !progress.files().contains(&selected) {
                    return Err("Selected file/viewed state changed; refresh Review progress before opening a new intent".into());
                }
                progress
                    .remember(&selected.path, database)
                    .map_err(|error| error.to_string())?;
                context
                    .workspace
                    .pr
                    .as_mut()
                    .ok_or("No exact PR is open")?
                    .review_progress = Some(progress);
                context
                    .workspace
                    .pr
                    .as_mut()
                    .unwrap()
                    .prepare_rows()
                    .map_err(|error| error.to_string())?;
            }
            if matches!(command, PrCommand::RemoveReaction | PrCommand::Subscription) {
                let loaded = if command == PrCommand::Subscription {
                    context.load_subscription(provider, cancel)
                } else {
                    context.load_reactions(provider, cancel)
                };
                match loaded {
                    Ok(()) => {}
                    Err(error) if account_failure(&error) => {
                        return Ok(PreparedView {
                            clear_private: true,
                            status: format!(
                                "PR account verification failed • private pages hidden: {error}"
                            ),
                            ..Default::default()
                        })
                    }
                    Err(error) => return Err(error),
                }
            }
            // Merge methods, queue, lock and branch state come from a fresh
            // GitHub read, never from the cached PR alone.
            let governed = matches!(
                command,
                PrCommand::Merge | PrCommand::EnableAutoMerge | PrCommand::DisableAutoMerge
            ) || PrCommand::GOVERNED.contains(&command);
            if governed && command.disabled_reason(&context).is_none() {
                match context.load_governance(command, provider, cancel) {
                    Ok(()) => {}
                    Err(error) if account_failure(&error) => {
                        return Ok(PreparedView {
                            clear_private: true,
                            status: format!(
                                "PR account verification failed • private pages hidden: {error}"
                            ),
                            ..Default::default()
                        })
                    }
                    Err(error) => return Err(error),
                }
            }
            let workspace = context.workspace.clone();
            if command == PrCommand::Subscription || governed {
                if let Some(reason) = command.disabled_reason(&context) {
                    return Ok(PreparedView {
                        context: Some(workspace.context()),
                        status: if governed {
                            format!(
                                "Current GitHub state read • {} is unavailable",
                                command.label()
                            )
                        } else {
                            "Current subscription read • changing it is unavailable".into()
                        },
                        github: Some(workspace),
                        readonly_notice: Some(reason),
                        ..Default::default()
                    });
                }
            }
            let mut form = Form::pr_action(command, context)?;
            if let Some(body) = Cache::open(database)
                .map_err(|e| e.to_string())?
                .draft(&form.scope(), &form.resource())
                .map_err(|e| e.to_string())?
            {
                form.restore_draft(&body)?;
            }
            Ok(PreparedView {
                context: Some(workspace.context()),
                status: "Local PR intent ready • nothing published; Preview before confirmation"
                    .into(),
                github: Some(workspace),
                form: Some(form),
                clear_pr_inspection: true,
                ..Default::default()
            })
        }
        PrUiAction::BrowseReactions => {
            // Browsing is deliberately permitted while a publication marker is
            // pending. This read does not inspect/acknowledge a write generation.
            match context.load_reactions(provider, cancel) {
                Ok(()) => {}
                Err(error) if account_failure(&error) => {
                    return Ok(PreparedView {
                        clear_private: true,
                        status: format!(
                            "PR reaction identity failed • private pages hidden: {error}"
                        ),
                        ..Default::default()
                    })
                }
                Err(error) => return Err(error),
            }
            let page = crate::pr_actions::PrReactionPage::from_context(&context)?;
            Ok(PreparedView {
                context: Some(context.workspace.context()),
                status: "Loaded exact comment reactions • read-only; no publication acknowledged"
                    .into(),
                github: Some(context.workspace),
                pr_reaction_display: Some(reaction_display(&page)),
                pr_reactions: Some(page),
                ..Default::default()
            })
        }
        PrUiAction::Inspect => {
            let journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
            let observation = match journal.prepare_inspection(
                provider,
                &context.identity,
                &context.target,
                cancel,
            ) {
                Ok(observation) => observation,
                Err(error) if account_failure(&error.to_string()) => {
                    return Ok(PreparedView {
                        clear_private: true,
                        status: format!(
                            "PR inspection identity failed • private pages hidden: {error}"
                        ),
                        ..Default::default()
                    })
                }
                Err(error) => return Err(error.to_string()),
            };
            let workspace = match context.workspace.replace_pr_details_with_current_context(
                observation.observation().details().clone(), provider, cancel,
            ) {
                Ok(workspace) => workspace,
                Err(error) => return Ok(PreparedView {
                    clear_private: true,
                    lines: vec!["Private PR metadata unavailable. Original publication markers remain unchanged; nothing was resent or acknowledged.".into()],
                    status: format!("PR inspection context unavailable • private pages hidden • no resend: {error}"),
                    clear_pr_inspection: true,
                    ..Default::default()
                }),
            };
            let pending = pending_message(database, &workspace)?;
            Ok(PreparedView { context:Some(workspace.context()), status:"Inspected exact PR result • read-only; choose Acknowledge inspected result explicitly".into(),github:Some(workspace),pr_inspection:Some(Arc::new(Mutex::new(Some(observation)))),pr_pending_message:Some(pending),..Default::default() })
        }
        PrUiAction::Acknowledge => {
            let slot =
                inspection.ok_or("Inspect this exact PR first; refresh is not acknowledgement.")?;
            let observation = slot
                .lock()
                .map_err(|_| "Inspection state unavailable; inspect again.")?
                .take()
                .ok_or("Inspection already consumed; inspect this exact PR again.")?;
            // Consume the opaque observation. A failed acknowledgement never creates a retry.
            let mut journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
            let count = match journal.acknowledge(provider, observation, &context.identity, cancel) {
                Ok(count) => count,
                Err(error) if account_failure(&error.to_string()) => return Ok(PreparedView {clear_private:true,status:format!("PR acknowledgement identity failed • private pages hidden; marker retained: {error}"),..Default::default()}),
                Err(error) => return Err(error.to_string()),
            };
            let pending = pending_message(database, &context.workspace)?;
            Ok(PreparedView { context:Some(context.workspace.context()),status:format!("Acknowledged inspected PR result • {count} pending marker(s) released; nothing resent"),github:Some(context.workspace),clear_pr_inspection:true,pr_pending_message:Some(pending),..Default::default() })
        }
    }
}

pub fn submit_native(
    mut form: Form,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::PrAction(intent) = &form.purpose else {
        return Err("Not a native PR intent".into());
    };
    let plan = intent
        .plan
        .clone()
        .ok_or("Choose Preview before confirmation")?;
    let workspace = intent.context.workspace.clone();
    let label = intent.command.label().to_string();
    let restored = form
        .restored_draft
        .then(|| form.publication.clone())
        .flatten();
    let mut journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
    let result = plan.execute(
        provider,
        &mut journal,
        true,
        restored.as_ref(),
        |marker| {
            form.publication = Some(marker.publication_reference());
            drafts.save_and_wait(form.scope(), form.resource(), form.draft())
        },
        cancel,
    );
    present(result, form, workspace, &label, drafts, database)
}

pub fn present(
    result: PrExecution,
    mut form: Form,
    mut workspace: GithubWorkspace,
    label: &str,
    drafts: &DraftClient,
    database: &Path,
) -> Result<PreparedView, String> {
    form.submitting = false;
    let mut clear_private = false;
    let confirmed;
    let status = match result {
        PrExecution::Refused { error } => {
            clear_private = account_failure(&error);
            form.error = error.clone();
            form.invalidate_plan();
            confirmed = false;
            format!("PR intent refused • {error}")
        }
        PrExecution::RestoredAlreadyConfirmed { marker } => {
            form.publication = Some(marker.publication_reference());
            confirmed = true;
            "Already published • this restored original attempt will not be resent".into()
        }
        PrExecution::UncertainWrite { publication, error } => {
            form.publication = Some(publication);
            form.error = format!(
                "Uncertain publication • {error} • inspect and acknowledge before retrying"
            );
            form.invalidate_plan();
            confirmed = false;
            form.error.clone()
        }
        PrExecution::Confirmed {
            workspace: updated,
            publication,
            effect,
        } => {
            let head = |workspace: &GithubWorkspace| {
                workspace
                    .pr
                    .as_ref()
                    .map(|page| page.details.summary.head_sha.clone())
                    .unwrap_or_default()
            };
            let prepared_head = head(&workspace);
            workspace = *updated;
            let moved = crate::pr_actions::refetched_head_note(&prepared_head, &head(&workspace));
            let note = moved
                .into_iter()
                .chain(effect.outcome_note())
                .map(|note| format!(" • {note}"))
                .collect::<String>();
            let cached = crate::pr_projection::persist_confirmed_snapshot(
                database,
                &workspace,
                &publication,
            );
            form.publication = Some(publication);
            confirmed = true;
            format!("Published • {label}{note}{}",cached.err().map_or(String::new(),|error|format!(" • derived search/cache refresh failed: {error}; publication is confirmed, do not resend")))
        }
        PrExecution::ConfirmedButRefreshFailed {
            publication,
            error,
            private_visible,
            ..
        } => {
            form.publication = Some(publication);
            confirmed = true;
            clear_private = !private_visible;
            if clear_private {
                format!("Published • private pages hidden • do not resend • {label} • refresh failed: {error}")
            } else {
                format!("Published • {label} • refresh failed • do not resend: {error}")
            }
        }
        PrExecution::ConfirmedButJournalFailed {
            publication, error, ..
        } => {
            form.publication = Some(publication);
            confirmed = false;
            form.error = format!("GitHub confirmed publication but local receipt failed: {error}. Do not resend; inspect and acknowledge the result.");
            form.invalidate_plan();
            form.error.clone()
        }
    };
    if form.publication.is_some() {
        form.dirty = false;
    }
    let status = if confirmed {
        form.submitted = true;
        form.dirty = false;
        // TASK-104: a late or lost cleanup ACK is "cleanup unconfirmed",
        // never a retained/deleted claim about the stored body.
        let cleanup = drafts.remove_confirmed_publication_v104(
            database,
            form.scope(),
            form.resource(),
            crate::drafts::CONFIRMED_CLEANUP_ACK_WAIT,
        );
        format!("{status} • {}", cleanup.caption())
    } else {
        status
    };
    // A presentation/storage read failure cannot reinterpret a confirmed remote
    // effect as a failed submission or leave its old Submit control active.
    let pending = pending_message(database, &workspace).unwrap_or_else(|error| format!("Publication state unavailable: {error}. Do not resend; inspect this exact PR before another write."));
    let invalidate_metrics = form.publication.is_some();
    Ok(PreparedView {
        lines: if clear_private && confirmed {
            vec![
                "Publication is confirmed. Do not resend this original request.".into(),
                "Private pages hidden after refresh/visibility failure. Verify access before reopening.".into(),
            ]
        } else {
            vec![]
        },
        context: (!clear_private).then(|| workspace.context()),
        status,
        github: (!clear_private).then_some(workspace),
        form: (!clear_private).then_some(form),
        clear_private,
        clear_pr_inspection: true,
        pr_pending_message: Some(pending),
        invalidate_metrics,
        ..Default::default()
    })
}
