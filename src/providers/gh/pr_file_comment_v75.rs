//! A genuine changed file is sufficient intent for a FILE comment. It is not
//! a line/range, an existing thread or permission to publish without closing
//! the caller's original source, SQL and current-principal bracket.
use super::*;
use std::{sync::Arc, time::Instant};

pub(crate) const FILE_COMMENT_BODY_CAP_V75: usize = 65_536;
pub(crate) const FILE_COMMENT_DRAFT_CAP_V75: usize = 128 * 1024;
pub(crate) const FILE_COMMENT_DOCUMENT_CAP_V75: usize = 128 * 1024;
pub(crate) const FILE_COMMENT_POSTWRITE_CAPS_V75: [usize; 7] = [
    256 * 1024,
    256 * 1024,
    256 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];
pub(crate) const FILE_COMMENT_NODE_QUERY_V75: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id fullDatabaseId subjectType body state path line startLine originalLine originalStartLine commit{oid} originalCommit{oid} replyTo{id fullDatabaseId} createdAt updatedAt author{__typename login avatarUrl ... on User{databaseId name} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";

/// Actual POST201 + exact numeric GET + opaque-node FILE observation. It is
/// not source permission and is never restored from durable JSON.
pub(crate) struct FileCommentReceiptV75 {
    comment: ConversationComment,
    principal: u64,
    path: String,
    head: String,
}
impl FileCommentReceiptV75 {
    pub(crate) fn comment(&self) -> &ConversationComment {
        &self.comment
    }
    pub(crate) fn principal(&self) -> u64 {
        self.principal
    }
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn head(&self) -> &str {
        &self.head
    }
}
impl std::fmt::Debug for FileCommentReceiptV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileCommentReceiptV75(<actual independent FILE observations; not causality>)")
    }
}

/// No public JSON/Deserialize or line-field constructor exists. The caller
/// already bounds the shared editor result before retaining this Arc.
pub(crate) struct FileCommentBodyV75(Arc<str>);
impl FileCommentBodyV75 {
    pub(crate) fn new(body: Arc<str>) -> GithubResult<Self> {
        if body.len() > FILE_COMMENT_BODY_CAP_V75 || body.trim().is_empty() {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        Ok(Self(body))
    }
    pub(crate) fn body(&self) -> &str {
        &self.0
    }
    pub(crate) fn payload(&self, original: &OriginalPrFileCommentV75) -> GithubResult<Value> {
        #[derive(Serialize)]
        struct Payload<'a> {
            body: &'a str,
            commit_id: &'a str,
            path: &'a str,
            subject_type: &'static str,
        }
        struct Bound(usize);
        impl std::io::Write for Bound {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|n| *n <= FILE_COMMENT_DRAFT_CAP_V75)
                    .ok_or_else(|| {
                        std::io::Error::other("File comment encoded body exceeds its bound")
                    })?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let payload = Payload {
            body: self.body(),
            commit_id: original.parent.head_sha(),
            path: &original.file.path,
            subject_type: "file",
        };
        serde_json::to_writer(Bound(0), &payload)
            .map_err(|_| safe(GithubErrorKind::InvalidTarget))?;
        serde_json::to_value(payload).map_err(|_| safe(GithubErrorKind::InvalidTarget))
    }
}
impl std::fmt::Debug for FileCommentBodyV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileCommentBodyV75(<bounded user body; no source or approval>)")
    }
}

/// This provider actually produced the original parent and unique member
/// under one measured allocation. Fields remain private; saved metadata and
/// form text cannot manufacture the producer or renew its clock.
pub(crate) struct OriginalPrFileCommentV75 {
    parent: PrResourceParentV1,
    repository: RepositoryCatalogueRepository,
    file: ObservedPrFileV1,
    principal: u64,
    issuer: GithubProvider,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for OriginalPrFileCommentV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalPrFileCommentV75(<actual measured FILE member; no write permit>)")
    }
}
impl OriginalPrFileCommentV75 {
    pub(crate) fn parent(&self) -> &PrResourceParentV1 {
        &self.parent
    }
    pub(crate) fn file(&self) -> &ObservedPrFileV1 {
        &self.file
    }
    pub(crate) fn principal(&self) -> u64 {
        self.principal
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.principal == other.principal
            && immutable_images::original_parent(&self.parent, &other.parent)
            && same_file(&self.file, &other.file)
    }
    pub(crate) fn check_measured(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Instant> {
        let effective = deadline.min(self.deadline);
        phase(cancel, effective)?;
        if !self.cancellation.same_instance(cancel)
            || !provider.runner.same_instance(&self.issuer.runner)
            || provider.host != self.issuer.host
            || provider.program != self.issuer.program
            || provider.env != self.issuer.env
            || !match (
                &provider.scoped_read_budget,
                &self.issuer.scoped_read_budget,
            ) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        self.issuer
            .scoped_read_budget
            .as_ref()
            .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?
            .check_clock(cancel)
            .map_err(GithubError::from)?;
        Ok(effective)
    }
    pub(in crate::providers::gh) fn ordinary_transport_v75(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<GithubProvider> {
        phase(cancel, self.deadline)?;
        if !self.cancellation.same_instance(cancel)
            || provider.scoped_read_budget.is_some()
            || !provider.runner.same_instance(&self.issuer.runner)
            || provider.host != self.issuer.host
            || provider.program != self.issuer.program
            || provider.env != self.issuer.env
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let mut bounded = provider.clone();
        bounded.timeout = bounded.timeout.min(self.issuer.timeout);
        let original = self
            .issuer
            .scoped_read_window
            .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
        // The immutable prepaid allowance retains the ORIGINAL ordinary
        // issuer's caps. The measured READ child cap is not a fresh WRITE cap;
        // retain any current narrower ordinary cap and narrow only the clock.
        Ok(bounded.with_catalogue_read_deadline(self.deadline.min(original.0)))
    }
}

fn phase(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(safe(GithubErrorKind::Transport))
    } else {
        Ok(())
    }
}
fn same_file(a: &ObservedPrFileV1, b: &ObservedPrFileV1) -> bool {
    // A patch flag is presentation availability, not FILE membership. File
    // comments work without a text patch, side, tree, blob download or line.
    a.path == b.path
        && a.previous_path == b.previous_path
        && a.status == b.status
        && a.blob_sha == b.blob_sha
}
fn bounded_issuer(provider: &GithubProvider) -> GithubResult<GithubProvider> {
    let capacity = std::mem::size_of::<GithubProvider>()
        .checked_add(provider.host.capacity())
        .and_then(|n| n.checked_add(provider.program.capacity()))
        .and_then(|n| {
            provider.env.iter().try_fold(n, |total, (key, value)| {
                total
                    .checked_add(key.capacity())?
                    .checked_add(value.capacity())?
                    .checked_add(128)
            })
        })
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
    if capacity > 16 * 1024 {
        return Err(safe(GithubErrorKind::InvalidTarget));
    }
    Ok(provider.clone())
}
fn bounded_repository(
    repository: &RepositoryCatalogueRepository,
) -> GithubResult<RepositoryCatalogueRepository> {
    let mut retained = std::mem::size_of::<RepositoryCatalogueRepository>()
        .saturating_add(repository.full_name.capacity())
        .saturating_add(
            repository
                .metadata_limitations
                .capacity()
                .saturating_mul(std::mem::size_of::<String>()),
        );
    for text in [
        &repository.node_id,
        &repository.description,
        &repository.default_branch,
        &repository.language,
        &repository.updated_at,
    ] {
        retained = retained.saturating_add(text.as_ref().map_or(0, String::capacity));
    }
    for text in &repository.metadata_limitations {
        retained = retained.saturating_add(text.capacity());
    }
    if let Some(owner) = &repository.owner {
        retained = retained
            .saturating_add(owner.node_id.as_ref().map_or(0, String::capacity))
            .saturating_add(owner.actor.login.capacity())
            .saturating_add(owner.actor.actor_type.capacity())
            .saturating_add(
                owner
                    .actor
                    .display_name
                    .as_ref()
                    .map_or(0, String::capacity),
            )
            .saturating_add(owner.actor.avatar_url.as_ref().map_or(0, String::capacity));
    }
    if retained > 256 * 1024 {
        return Err(safe(GithubErrorKind::InvalidTarget));
    }
    Ok(repository.clone())
}
fn bigint(value: &Value) -> Option<u64> {
    match value {
        Value::Number(_) => value
            .as_u64()
            .filter(|id| *id > 0 && *id <= i64::MAX as u64),
        Value::String(raw)
            if !raw.is_empty()
                && raw.len() <= 20
                && !raw.starts_with('0')
                && raw.bytes().all(|b| b.is_ascii_digit()) =>
        {
            raw.parse::<u64>()
                .ok()
                .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        }
        _ => None,
    }
}
fn actual_text(raw: &Value, field: &str, cap: usize) -> bool {
    raw.get(field)
        .is_none_or(|value| value.is_null() || value.as_str().is_some_and(|s| s.len() <= cap))
}
fn rest_matches(
    original: &OriginalPrFileCommentV75,
    body: &FileCommentBodyV75,
    raw: &Value,
    id: u64,
    node: &str,
) -> bool {
    let parent = &original.parent;
    let endpoint = format!("repos/{}/pulls/{}", parent.repository(), parent.number());
    raw["id"].as_u64() == Some(id)
        && id > 0
        && id <= i64::MAX as u64
        && opaque(node)
        && raw["node_id"].as_str() == Some(node)
        && raw["pull_request_url"].as_str().is_some_and(|url| {
            canonical_source_url_matches(&original.issuer.host, url, &endpoint, true, false)
        })
        && raw["path"].as_str() == Some(original.file.path.as_str())
        && raw["commit_id"].as_str() == Some(parent.head_sha())
        && raw["original_commit_id"].as_str() == Some(parent.head_sha())
        && raw["body"].as_str() == Some(body.body())
        && actual_text(raw, "html_url", 4096)
        && raw["user"]["id"].as_u64() == Some(original.principal)
        && ["login", "name", "avatar_url", "type"]
            .iter()
            .all(|field| actual_text(&raw["user"], field, 4096))
        && raw.get("in_reply_to_id").is_none_or(Value::is_null)
        && raw
            .get("subject_type")
            .is_none_or(|value| value.as_str() == Some("file"))
        && ["line", "start_line", "original_line", "original_start_line"]
            .iter()
            .all(|field| raw.get(*field).is_none_or(Value::is_null))
        && ["created_at", "updated_at"].iter().all(|field| {
            raw[*field].as_str().is_some_and(|s| {
                s.len() <= 128 && crate::github_workspace::timestamp_seconds(s).is_some()
            })
        })
}
fn node_matches(
    original: &OriginalPrFileCommentV75,
    body: &FileCommentBodyV75,
    value: &Value,
    rest: &Value,
    id: u64,
    node: &str,
) -> bool {
    if value.get("errors").is_some_and(|errors| {
        !errors.is_null() && errors.as_array().is_none_or(|errors| !errors.is_empty())
    }) {
        return false;
    }
    let raw = &value["data"]["node"];
    let parent = &original.parent;
    let repo = &raw["repository"];
    let pr = &raw["pullRequest"];
    raw["__typename"] == "PullRequestReviewComment"
        && raw["id"].as_str() == Some(node)
        && bigint(&raw["fullDatabaseId"]) == Some(id)
        && raw["subjectType"] == "FILE"
        && raw["body"].as_str() == Some(body.body())
        && raw["state"] == "SUBMITTED"
        && raw["path"].as_str() == Some(original.file.path.as_str())
        && raw["commit"]["oid"].as_str() == Some(parent.head_sha())
        && raw["originalCommit"]["oid"].as_str() == Some(parent.head_sha())
        && raw.get("replyTo") == Some(&Value::Null)
        && ["line", "startLine", "originalLine", "originalStartLine"]
            .iter()
            .all(|field| raw.get(*field) == Some(&Value::Null))
        && raw["author"]["databaseId"].as_u64() == Some(original.principal)
        && ["login", "name", "avatarUrl", "__typename"]
            .iter()
            .all(|field| actual_text(&raw["author"], field, 4096))
        && raw["createdAt"] == rest["created_at"]
        && raw["updatedAt"] == rest["updated_at"]
        && bigint(&raw["pullRequestReview"]["fullDatabaseId"])
            == rest["pull_request_review_id"].as_u64().filter(|id| *id > 0)
        && raw["pullRequestReview"]["id"].as_str().is_some_and(opaque)
        && raw["pullRequestReview"]["state"] == "COMMENTED"
        && repo["id"].as_str() == Some(parent.repository_node())
        && repo["databaseId"].as_u64() == Some(parent.repository_id())
        && repo["nameWithOwner"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(parent.repository()))
        && pr["repository"] == *repo
        && pr["id"].as_str() == Some(parent.node_id())
        && bigint(&pr["fullDatabaseId"]) == Some(parent.database_id())
        && pr["number"].as_u64() == Some(parent.number())
        && pr["headRefOid"].as_str() == Some(parent.head_sha())
        && pr["baseRefOid"].as_str() == Some(parent.base_tip_sha())
}

impl GithubProvider {
    /// P+2 actual bounded READ children, and no raw patch request. The caller
    /// must reserve final prePOST user/parent plus Core repository/principal
    /// closure separately under THIS same allocation/token/absolute deadline.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_file_comment_original_v75(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        original: &PrResourceReadV1,
        selected: usize,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        original_deadline: Instant,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<OriginalPrFileCommentV75> {
        limits.validate()?;
        let budget = self
            .scoped_read_budget
            .as_ref()
            .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
        let deadline = original_deadline.min(budget.deadline()).min(
            self.scoped_read_window
                .map_or(original_deadline, |(clock, _)| clock),
        );
        phase(cancel, deadline)?;
        budget.check_clock(cancel).map_err(GithubError::from)?;
        let expected = original
            .files()
            .get(selected)
            .filter(|file| !file.path.is_empty() && file.path.len() <= 4096)
            .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
        if original.kind() != PrResourceKindV1::Files
            || identity.host != self.host
            || identity
                .account_id
                .is_none_or(|uid| uid == 0 || uid > i64::MAX as u64)
            || current.repository().permissions.pull != Some(true)
            || current.repository().id != original.parent().repository_id()
            || current.repository().node_id.as_deref() != Some(original.parent().repository_node())
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let issuer = bounded_issuer(self)?;
        let repository = bounded_repository(current.repository())?;
        let mut limits = limits.clone();
        limits.max_document_bytes = limits.max_document_bytes.min(FILE_COMMENT_DOCUMENT_CAP_V75);
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            original.parent().number()
        );
        let opening = parent(
            self,
            current,
            original.parent().summary(),
            &document(self, &endpoint, limits.max_document_bytes, cancel, false)?,
        )?;
        if !immutable_images::original_parent(original.parent(), &opening) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        after_opening(&opening)?;
        phase(cancel, deadline)?;
        let (files, coverage) = collect_files(self, &opening, &limits, cancel)?;
        if !coverage.complete_observed() {
            return Err(safe(GithubErrorKind::Unsupported));
        }
        let mut members = files.iter().filter(|file| file.path == expected.path);
        let file = members
            .next()
            .filter(|_| members.next().is_none())
            .filter(|file| same_file(expected, file))
            .ok_or_else(|| safe(GithubErrorKind::StaleHead))?
            .clone();
        let closing = parent(
            self,
            current,
            opening.summary(),
            &document(self, &endpoint, limits.max_document_bytes, cancel, true)?,
        )?;
        if !same_parent(&opening, &closing) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        checked_finished(self, cancel)?;
        phase(cancel, deadline)?;
        Ok(OriginalPrFileCommentV75 {
            parent: closing,
            repository,
            file,
            principal: identity
                .account_id
                .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?,
            issuer,
            cancellation: cancel.clone(),
            deadline,
        })
    }

    /// Exactly TWO measured children: final user64KiB + original rawPR128KiB.
    /// The owning SAME Core then closes its actual repository/SQL/final UID;
    /// this pure parser reuse does not add a third repository HTTP request.
    pub(crate) fn file_comment_last_parent_v75(
        &self,
        original: &OriginalPrFileCommentV75,
        identity: &HostIdentity,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let deadline = original.check_measured(self, cancel, deadline)?;
        if identity.host != self.host || identity.account_id != Some(original.principal) {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let mut bounded = self.clone();
        bounded.timeout = bounded.timeout.min(original.issuer.timeout);
        let user = document(&bounded, "user", 64 * 1024, cancel, false)?;
        if user["id"].as_u64() != Some(original.principal) {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let raw = document(
            &bounded,
            &format!(
                "repos/{}/pulls/{}",
                original.parent.repository(),
                original.parent.number()
            ),
            FILE_COMMENT_DOCUMENT_CAP_V75,
            cancel,
            true,
        )?;
        let current = parent_against_repository_v75(
            &bounded,
            &original.repository,
            original.parent.summary(),
            &raw,
        )?;
        if !same_parent(&original.parent, &current) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        phase(cancel, deadline)
    }

    /// One invocation. All source/member/user/head/SQL guards precede actual
    /// close and write; every later failure remains protected uncertainty.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_file_comment_guarded_v75(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        expected: &OriginalPrFileCommentV75,
        fresh: &OriginalPrFileCommentV75,
        identity: &HostIdentity,
        body: &FileCommentBodyV75,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: impl FnOnce() -> GithubResult<()>,
    ) -> Result<FileCommentReceiptV75, ThreadPublicationFailureV1> {
        let mut invoked = false;
        let result = (|| {
            let deadline = fresh
                .check_measured(measured, cancel, deadline)?
                .min(captures.deadline());
            let ordinary = fresh.ordinary_transport_v75(self, cancel)?;
            captures.validate_prewrite(measured, cancel)?;
            if !expected.same_observation(fresh)
                || identity.host != self.host
                || identity.account_id != Some(fresh.principal)
            {
                return Err(safe(GithubErrorKind::StaleHead));
            }
            // Validate encoded bytes BEFORE source closure and transport.
            let _ = body.payload(fresh)?;
            measured.file_comment_last_parent_v75(fresh, identity, cancel, deadline)?;
            before_write()?;
            phase(cancel, deadline)?;
            invoked = true;
            let created = captures.file_comment_created201_v75(&ordinary, fresh, body, cancel)?;
            let raw = created.value();
            let id = raw["id"]
                .as_u64()
                .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                .ok_or_else(|| safe(GithubErrorKind::AmbiguousWrite))?;
            let node = raw["node_id"]
                .as_str()
                .filter(|node| opaque(node))
                .ok_or_else(|| safe(GithubErrorKind::AmbiguousWrite))?;
            if !rest_matches(fresh, body, raw, id, node) {
                return Err(safe(GithubErrorKind::AmbiguousWrite));
            }
            let readback = captures.api(
                &ordinary,
                &format!("repos/{}/pulls/comments/{id}", fresh.parent.repository()),
                "GET",
                None,
                None,
                cancel,
            )?;
            if !rest_matches(fresh, body, &readback, id, node)
                || raw["created_at"] != readback["created_at"]
                || raw["updated_at"] != readback["updated_at"]
                || raw["pull_request_review_id"] != readback["pull_request_review_id"]
            {
                return Err(safe(GithubErrorKind::AmbiguousWrite));
            }
            let gql = captures.api(
                &ordinary,
                "graphql",
                "POST",
                Some(json!({"query":FILE_COMMENT_NODE_QUERY_V75,"variables":{"id":node}})),
                None,
                cancel,
            )?;
            if !node_matches(fresh, body, &gql, &readback, id, node) {
                return Err(safe(GithubErrorKind::AmbiguousWrite));
            }
            phase(cancel, deadline)?;
            let result = FileCommentReceiptV75 {
                comment: comment_from_json(&readback),
                principal: fresh.principal,
                path: fresh.file.path.clone(),
                head: fresh.parent.head_sha().into(),
            };
            phase(cancel, deadline)?;
            Ok(result)
        })();
        result.map_err(|error| {
            if invoked {
                self.runner.mark_ambiguous(
                    &self.resource(fresh.parent.repository(), fresh.parent.number()),
                );
                ThreadPublicationFailureV1::WriteMayHaveStarted(error)
            } else {
                ThreadPublicationFailureV1::NotStarted(error)
            }
        })
    }

    /// Remaining SAME four prepaid slots; local cleanup is bracketed before
    /// final UID and the terminal discussion fence, never followed by a reload.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn close_file_comment_visibility_v75(
        &self,
        original: &OriginalPrFileCommentV75,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        local: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<()> {
        let deadline = deadline.min(original.deadline).min(captures.deadline());
        phase(cancel, deadline)?;
        let ordinary = original.ordinary_transport_v75(self, cancel)?;
        let opening = captures.api(&ordinary, "user", "GET", None, None, cancel)?;
        if opening["id"].as_u64() != Some(original.principal) {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        ordinary.pr_thread_postwrite_parent_v2(current, &original.parent, captures, cancel)?;
        phase(cancel, deadline)?;
        local()?;
        phase(cancel, deadline)?;
        let closing = captures.api(&ordinary, "user", "GET", None, None, cancel)?;
        if closing["id"].as_u64() != Some(original.principal) {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        phase(cancel, deadline)
    }
}
