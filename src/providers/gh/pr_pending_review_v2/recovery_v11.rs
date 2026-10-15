//! Fresh current OUTCOME observations, not mutation receipts or capabilities.
//! Only the original measured producer can issue one; no serialized constructor.
use super::*;
use crate::providers::runner::CommandRunner;

pub struct ReviewRecoveryCurrentV11 {
    issuer: CommandRunner,
    host: String,
    program: std::ffi::OsString,
    environment: BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    parent: Arc<ReviewParentV2>,
    identity: HostIdentity,
    resource: String,
    generation: u64,
    cancellation: CancellationToken,
    deadline: Instant,
    catalogue: ReviewCatalogueV2,
    selected: Option<ReviewSnapshotV2>,
    lines: Arc<[String]>,
}
impl std::fmt::Debug for ReviewRecoveryCurrentV11 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ReviewRecoveryCurrentV11(<actual point-in-time outcome; no publication permit>)",
        )
    }
}
impl ReviewRecoveryCurrentV11 {
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn matches_target(&self, target: &crate::pr_write_journal::PrWriteTarget) -> bool {
        target.scope().host == self.parent.host()
            && target.scope().login == format!("user-id:{}", self.parent.observed_by())
            && target.node_id() == self.parent.pr_node()
            && target.number() == self.parent.number()
            && target.repository() == self.parent.repository()
    }
    pub(crate) fn validate(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        phase(cancel, deadline.min(self.deadline))?;
        phase(&self.cancellation, deadline.min(self.deadline))?;
        if provider.host != self.host
            || provider.program != self.program
            || provider.env != self.environment
            || !provider.runner.same_instance(&self.issuer)
            || !self.catalogue.complete()
            || self
                .selected
                .as_ref()
                .is_some_and(|snapshot| !snapshot.complete())
        {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Original review inspection issuer or complete observation changed",
            ));
        }
        let generation = provider
            .runner
            .review_resource_generation_v11(&self.resource, cancel, deadline.min(self.deadline))
            .map_err(|_| {
                refusal(
                    GithubErrorKind::StaleHead,
                    "Original review inspection expired, cancelled or predates another write",
                )
            })?;
        if generation != self.generation {
            return Err(refusal(
                GithubErrorKind::StaleHead,
                "A newer write retired this review inspection",
            ));
        }
        Ok(())
    }
}
impl GithubProvider {
    // This boundary keeps original identity, parent, purpose, limits, token and
    // absolute deadline distinct; grouping them must not mint a new authority.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn inspect_review_outcome_v11(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        selected: Option<(u64, Option<&str>)>,
        requested: bool,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ReviewRecoveryCurrentV11> {
        let budget = self.scoped_read_budget.as_ref().ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Review recovery needs its genuine original measured session",
            )
        })?;
        let deadline = deadline.min(budget.deadline()).min(
            self.scoped_read_window
                .map_or(deadline, |(original, _)| original),
        );
        phase(cancel, deadline)?;
        limits.validate()?;
        let original = ReviewParentV2::capture(identity, parent)?;
        let resource = self.resource(parent.repository(), parent.number());
        let generation = self
            .runner
            .review_resource_generation_v11(&resource, cancel, deadline)
            .map_err(|_| {
                refusal(
                    GithubErrorKind::Transport,
                    "Original write resource is busy or recovery deadline ended",
                )
            })?;
        let catalogue = self.review_catalogue_v2(identity, parent, limits, cancel, deadline)?;
        if !catalogue.complete() {
            return Err(refusal(
                GithubErrorKind::Unsupported,
                "Current review enumeration is incomplete; no acknowledgement observation issued",
            ));
        }
        let mut lines=vec!["FRESH CURRENT OUTCOME • point-in-time observation, NOT proof of this attempt's causality".into(),
            format!("Current H {} • historical H is shown separately",parent.head_sha()),
            format!("Complete bounded current reviews: {} • numeric inspector UID {:?}",catalogue.rows().len(),identity.account_id)];
        let mut snapshot = None;
        if let Some((id, node)) = selected {
            let row = catalogue.rows().iter().find(|row| row.id() == id);
            match row {
                Some(row) if node.is_none_or(|expected|row.node()==expected)=>{
                    let observed=self.review_snapshot_v2(identity,parent,row,limits,cancel,deadline)?;
                    if !observed.complete(){return Err(refusal(GithubErrorKind::Unsupported,"Selected current review comments are incomplete"));}
                    let mut borrowed=observed.review().body().len();
                    for comment in observed.comments(){phase(cancel,deadline)?;
                        borrowed=borrowed.checked_add(comment.body().len()).and_then(|bytes|bytes.checked_add(comment.node().len()+comment.commit().len()+4096))
                            .filter(|bytes|*bytes<=7*1024*1024).ok_or_else(||refusal(GithubErrorKind::Unsupported,
                                "Current comment bodies exceed bounded recovery presentation before cloning"))?;
                    }
                    let author=crate::identity::ActorIdentity::github(&self.host,row.author());
                    lines.push(format!("Current review #{} / {} • {:?} • [{}] {} • current author UID {:?}",row.id(),row.node(),row.state(),author.initials(),author.label(),row.author_account_id()));
                    if node.is_none(){lines.push("Original opaque review node was never recorded; current numeric match is NOT original identity or causality proof".into());}
                    lines.push(format!("Current review summary: {}",observed.review().body()));
                    for comment in observed.comments(){phase(cancel,deadline)?;
                        let author=crate::identity::ActorIdentity::github(&self.host,comment.author());
                        lines.push(format!("Current comment #{} / {} • [{}] {} • current author UID {:?} • commit {}",comment.id(),comment.node(),author.initials(),author.label(),comment.author_account_id(),comment.commit()));
                        lines.push(comment.body().into());
                    }
                    snapshot=Some(observed);
                }
                Some(_)=>return Err(refusal(GithubErrorKind::InvalidTarget,"Current review numeric ID maps to a different original opaque node")),
                None=>lines.push(format!("Complete current catalogue contains no review #{id}; absence observed, not server causality or failure proof")),
            }
        } else {
            lines.push("Original created review identity was never observed. Attribution is UNKNOWN; no first-PENDING guess or digest inversion".into());
            for row in catalogue.rows() {
                phase(cancel, deadline)?;
                let author = crate::identity::ActorIdentity::github(&self.host, row.author());
                lines.push(format!(
                    "Current review #{} / {} • {:?} • [{}] {} • actual author UID {:?}",
                    row.id(),
                    row.node(),
                    row.state(),
                    author.initials(),
                    author.label(),
                    row.author_account_id()
                ));
            }
        }
        if requested {
            let observed = rerequest_v3::read::requested(self, &original, cancel, deadline)?;
            lines.push("Current requested membership ONLY; notification delivery and this attempt's causality UNKNOWN".into());
            for (kind, id, node) in observed.iter() {
                lines.push(format!("Currently requested {kind:?} #{id} / {node}"));
            }
        }
        if lines
            .iter()
            .try_fold(0usize, |sum, line| sum.checked_add(line.len()))
            .is_none_or(|bytes| bytes > 8 * 1024 * 1024)
        {
            return Err(refusal(
                GithubErrorKind::Unsupported,
                "Bounded current recovery display exceeds8MiB",
            ));
        }
        phase(cancel, deadline)?;
        let observed = ReviewRecoveryCurrentV11 {
            issuer: self.runner.clone(),
            host: self.host.clone(),
            program: self.program.clone(),
            environment: self.env.clone(),
            parent: original,
            identity: identity.clone(),
            resource,
            generation,
            cancellation: cancel.clone(),
            deadline,
            catalogue,
            selected: snapshot,
            lines: lines.into(),
        };
        observed.validate(self, cancel, deadline)?;
        Ok(observed)
    }
    pub(crate) fn validate_review_inspection_v11(
        &self,
        observed: &ReviewRecoveryCurrentV11,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        observed.validate(self, cancel, deadline)?;
        if observed.parent.as_ref() != ReviewParentV2::capture(identity, parent)?.as_ref()
            || observed.identity.host != identity.host
            || observed.identity.account_id != identity.account_id
        {
            return Err(refusal(
                GithubErrorKind::IdentityChanged,
                "Fresh current review inspection belongs to another numeric account/parent",
            ));
        }
        Ok(())
    }
    pub(crate) fn reconcile_review_inspection_v11(
        &self,
        observed: ReviewRecoveryCurrentV11,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        observed.validate(self, cancel, deadline)?;
        self.runner
            .reconcile_review_generation_v11(
                &observed.resource,
                observed.generation,
                cancel,
                deadline.min(observed.deadline),
            )
            .map_err(|_| {
                refusal(
                    GithubErrorKind::StaleHead,
                    "Review inspection expired or a newer write still requires inspection",
                )
            })
    }
}
