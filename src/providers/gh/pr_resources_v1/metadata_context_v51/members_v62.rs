//! Fixed Members context/record extension. Only the genuine typed publisher or
//! fresh Members read can issue the private command proof; this is display data.
use super::*;
use crate::{
    pr_metadata_actions::{PrMetadataCommand, PrMetadataPlan},
    providers::gh::OriginalMetadataMembersV48,
};

/// Metadata-Members only. The inherited scalar comes from the same actual
/// retained producer/plan, never a replacement transport or model allowance.
pub(in crate::providers::gh) struct MetadataMembersHeapV62(HeapV51, bool);
impl MetadataMembersHeapV62 {
    pub(in crate::providers::gh) fn new(existing: usize) -> GithubResult<Self> {
        let bytes = existing
            .checked_add(3 * CHILD_CAP_V51)
            .filter(|n| *n <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(Self(
            HeapV51 {
                bytes,
                nodes: 0,
                existing,
            },
            false,
        ))
    }
    pub(in crate::providers::gh) fn object(&mut self, bytes: &[u8]) -> GithubResult<Value> {
        let before = self.0.bytes;
        let raw = decode(bytes, &mut self.0)?;
        self.before_mapping(before)?;
        Ok(raw)
    }
    pub(in crate::providers::gh) fn eligible_page(&mut self, bytes: &[u8]) -> GithubResult<Value> {
        if bytes.len() > CHILD_CAP_V51 {
            return Err(refused(GithubErrorKind::Transport));
        }
        let before = self.0.bytes;
        let mut de = serde_json::Deserializer::from_slice(bytes);
        let raw = SeedV51 {
            heap: &mut self.0,
            depth: 0,
        }
        .deserialize(&mut de)
        .map_err(|_| refused(GithubErrorKind::Json))?;
        de.end().map_err(|_| refused(GithubErrorKind::Json))?;
        if !raw.is_array() {
            return Err(refused(GithubErrorKind::Json));
        }
        self.before_mapping(before)?;
        Ok(raw)
    }
    pub(in crate::providers::gh) fn detail_pages(
        &mut self,
        bytes: &[u8],
    ) -> GithubResult<Vec<Value>> {
        const LIST_CAP: usize = 512 * 1024;
        if bytes.len() > LIST_CAP {
            return Err(refused(GithubErrorKind::Transport));
        }
        if !self.1 {
            // The fixed five-list phase can retain a 512KiB runner output and
            // serde escape/reallocation scratch, versus the 128KiB fixed-role
            // scratch already charged by new(). Reserve the difference ONCE
            // before decoding the first list; the monotonic debit never resets.
            self.0
                .charge::<serde_json::Error>(3 * (LIST_CAP - CHILD_CAP_V51))
                .map_err(|_| refused(GithubErrorKind::Transport))?;
            self.1 = true;
        }
        let before = self.0.bytes;
        let mut de = serde_json::Deserializer::from_slice(bytes);
        let raw = SeedV51 {
            heap: &mut self.0,
            depth: 0,
        }
        .deserialize(&mut de)
        .map_err(|_| refused(GithubErrorKind::Json))?;
        de.end().map_err(|_| refused(GithubErrorKind::Json))?;
        let pages = raw
            .as_array()
            .filter(|pages| !pages.is_empty())
            .ok_or_else(|| refused(GithubErrorKind::Json))?;
        let rows = pages
            .iter()
            .try_fold(0usize, |n, page| {
                page.as_array().and_then(|items| n.checked_add(items.len()))
            })
            .filter(|n| *n <= 10_000)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        // Before any all-pages Vec growth, three borrowed identity-set inserts,
        // or comment/commit/file DTO mapping. Seed charges actual raw Value,
        // key/string and map allocations; two tree equivalents cover the DTO
        // conversion while raw trees still live. These row terms additionally
        // cover identity-set nodes and geometric Vec reallocation overlap.
        self.before_mapping(before)?;
        self.0
            .charge::<serde_json::Error>(
                rows.checked_mul(384 + 2 * std::mem::size_of::<Value>())
                    .ok_or_else(|| refused(GithubErrorKind::Transport))?,
            )
            .map_err(|_| refused(GithubErrorKind::Transport))?;
        GithubProvider::api_list_pages_v19(raw, None)
    }
    fn before_mapping(&mut self, before: usize) -> GithubResult<()> {
        // BEFORE collect_reader clones admitted candidate Values or observed_
        // context converts/clones the PR/repository/actors. The Seed has charged
        // every actual String capacity, Value/Vec slot and BTreeMap key/node.
        // Two additional tree equivalents bound the retained candidate clone
        // plus typed/current/opposite-set and parent/repository DTO conversions.
        // This monotonic same4MiB debit is never reset between actual pages.
        let retained = self
            .0
            .bytes
            .checked_sub(before)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        self.0
            .charge::<serde_json::Error>(retained)
            .map_err(|_| refused(GithubErrorKind::Transport))
    }
    pub(in crate::providers::gh) fn retained_bound(&self) -> usize {
        self.0.bytes
    }
    /// Consume the completed fixed-role Original read, not a live/detail heap.
    /// Each fixed child has returned only its decoded Value: its runner output
    /// and serde scratch have already dropped. Keep every inherited producer,
    /// plan, raw-tree and mapping charge, but do not inherit this phase's dead
    /// 3x128KiB capture scratch into the next Original read. The live monotonic
    /// limit above was enforced before every allocation and is never reset.
    pub(in crate::providers::gh) fn finish_original_read_v80(self) -> GithubResult<usize> {
        if self.1 {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        self.0
            .bytes
            .checked_sub(3 * CHILD_CAP_V51)
            .filter(|bytes| *bytes >= self.0.existing && *bytes <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))
    }
}

#[cfg(test)]
#[path = "persistent_v80_tests.rs"]
mod persistent_v80_tests;

impl ActualMetadataTabContextV51 {
    pub(crate) fn reserve_members_blocked_section_v62(&mut self) -> GithubResult<()> {
        // The first source_blocking read is bounded to8KiB and reuses this
        // context's already retained3x128KiB transport/serde scratch debit.
        // A blocked branch then reserves256KiB BEFORE origin/witness capture:
        // four16KiB origin raw/typed/encoded/serde copies (64KiB), four8KiB
        // marker/raw-row copies (32KiB), two32KiB target/path/SQL/value/struct
        // terms (64KiB), plus two16KiB encoder growth copies (32KiB) and64KiB
        // fixed statement/map/validation overlap. No protected body is loaded.
        // Existing exact capture limits stay16KiB origin/8KiB marker. Ordinary
        // None never invokes this method, preserving its previous acceptance.
        self.retained_bytes = self
            .retained_bytes
            .checked_add(256 * 1024)
            .filter(|bytes| *bytes <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(())
    }
    fn seal_members_v62(
        mut self,
        actual: &OriginalMetadataMembersV48,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<Self> {
        if !snapshot.metadata_context_matches_v51(&self)? {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        self.parent = Arc::clone(snapshot.parent());
        let uid = actual
            .identity()
            .account_id
            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
        let producer_overlap = actual
            .retained_model_v62()?
            .checked_add(64 * 1024)
            .filter(|bytes| *bytes <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        // Keep actual context bytes separate: it is the context alone that
        // coexists with its Value/record/encoded representations. The original
        // producer/plan remains alive once, not twelve serialized context copies.
        self.members_v62 = Some((actual.command(), uid, producer_overlap));
        self.validate_members_snapshot_v62(actual.command(), snapshot)?;
        Ok(self)
    }
    pub(crate) fn members_command_v62(&self) -> Option<PrMetadataCommand> {
        self.members_v62.map(|(command, _, _)| command)
    }
    pub(crate) fn members_producer_overlap_v62(
        &self,
        command: PrMetadataCommand,
    ) -> GithubResult<usize> {
        self.members_v62
            .filter(|(actual, _, _)| *actual == command)
            .map(|(_, _, bytes)| bytes)
            .filter(|bytes| *bytes <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))
    }
    pub(crate) fn validate_members_snapshot_v62(
        &self,
        command: PrMetadataCommand,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<()> {
        if self.members_command_v62() != Some(command)
            || !Arc::ptr_eq(&self.parent, snapshot.parent())
            || !snapshot.metadata_context_matches_v51(self)?
            || snapshot
                .metadata_model_bytes_v51()?
                .checked_add(self.retained_bytes)
                .and_then(|bytes| {
                    bytes.checked_add(self.members_producer_overlap_v62(command).ok()?)
                })
                .is_none_or(|bytes| bytes > MODEL_CAP_V51)
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(crate) fn members_encoded_bytes_v62(
        &self,
        command: PrMetadataCommand,
    ) -> GithubResult<usize> {
        if self.members_command_v62() != Some(command) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|n| *n <= 512 * 1024)
                    .ok_or_else(|| std::io::Error::other("Actual Members context encoded cap"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, &self.context)
            .map_err(|_| refused(GithubErrorKind::Transport))?;
        Ok(count.0)
    }
    pub(crate) fn members_context_copy_bytes_v62(
        &self,
        command: PrMetadataCommand,
    ) -> GithubResult<usize> {
        if self.members_command_v62() != Some(command) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // Actual retained context fields only. retained_bytes additionally
        // reserves the fixed runner/serde scratch and summary/facts/row peak;
        // those live once and must not be counted as twelve context clones.
        crate::pr_metadata_choices::members_context_capacity_v48(&self.context)
            .ok_or_else(|| refused(GithubErrorKind::Transport))
    }
    pub(crate) fn prepare_members_record_v62(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
        target: &crate::pr_write_journal::PrWriteTarget,
        command: PrMetadataCommand,
    ) -> GithubResult<crate::storage::Record> {
        self.validate_members_snapshot_v62(command, snapshot)?;
        let (_, uid, _) = self
            .members_v62
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
        if crate::storage::Scope::github(&self.summary.pr.host, Some(uid))
            .map_err(|_| refused(GithubErrorKind::IdentityChanged))?
            != *target.scope()
            || target.node_id() != self.parent.node_id()
            || target.number() != self.parent.number()
            || !target
                .repository()
                .eq_ignore_ascii_case(self.parent.repository())
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.members_encoded_bytes_v62(command)?;
        let mut record = crate::storage::Record::new(
            "pr-metadata-context-v48",
            target.node_id(),
            target.repository(),
            serde_json::to_value(&self.context).map_err(|_| refused(GithubErrorKind::Json))?,
        );
        record.complete = true;
        record.source_updated_at = Some(snapshot.details().summary.pr.updated_at.clone());
        Ok(record)
    }
}

impl MetadataContextDocumentsV51 {
    pub(in crate::providers::gh) fn finish_members_fresh_v62(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &OriginalMetadataMembersV48,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent = parse::parent(provider, current, snapshot.parent().summary(), &first)?;
        if !parse::same_parent(&parent, snapshot.parent())
            || !parse::same_parent(actual.parent(), snapshot.parent())
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        provider.validate_members_fresh_context_v62(actual, &first, &issue)?;
        bind_members_v82(
            provider,
            current,
            Arc::clone(snapshot.parent()),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )?
        .seal_members_v62(actual, snapshot)
    }
    pub(in crate::providers::gh) fn finish_members_paid_v62(
        mut self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &OriginalMetadataMembersV48,
        plan: &PrMetadataPlan,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        if !actual.metadata_tab_v62()
            || actual.command() != plan.command()
            || !parse::same_parent(actual.parent(), snapshot.parent())
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent = parse::parent(provider, current, snapshot.parent().summary(), &first)?;
        if !parse::same_parent(&parent, snapshot.parent()) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // REST validation allocates at most two borrowed-ID sets; charge their
        // actual rows BEFORE that validator, while all four raw trees live.
        let rows = [
            (&issue, "labels"),
            (&issue, "assignees"),
            (&first, "requested_reviewers"),
            (&first, "requested_teams"),
        ]
        .into_iter()
        .try_fold(0usize, |n, (raw, key)| {
            n.checked_add(raw[key].as_array().map_or(0, Vec::len))
        })
        .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        self.heap
            .charge::<serde_json::Error>(
                rows.checked_mul(512)
                    .and_then(|n| n.checked_add(8 * 1024))
                    .ok_or_else(|| refused(GithubErrorKind::Transport))?,
            )
            .map_err(|_| refused(GithubErrorKind::Transport))?;
        provider.validate_members_context_after_v62(actual, plan, &first, &issue)?;
        let context = bind_members_v82(
            provider,
            current,
            Arc::clone(snapshot.parent()),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )?;
        context.seal_members_v62(actual, snapshot)
    }
}

impl GithubProvider {
    pub(in crate::providers::gh) fn pr_members_metadata_refresh_context_v62(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &OriginalMetadataMembersV48,
        snapshot: &ConversationDetailSnapshotV19,
        retained: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        if !actual.metadata_tab_v62() || !parse::same_parent(actual.parent(), snapshot.parent()) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let parent = snapshot.parent();
        let endpoints = [
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            format!("repos/{}", parent.repository()),
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
        ];
        let mut documents = MetadataContextDocumentsV51::new(retained)?;
        for endpoint in endpoints {
            checked_clock_only_v51(self, cancel)?;
            let mut spec = self.spec(vec![
                "api".into(),
                endpoint,
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
            ]);
            spec.output_limit = spec.output_limit.min(CHILD_CAP_V51);
            let output = self
                .runner
                .run(spec, cancel)
                .map_err(|failure| refused(GithubError::from(failure).kind))?;
            checked_clock_only_v51(self, cancel)?;
            documents.accept_fixed_document(&output.stdout)?;
        }
        documents.finish_members_fresh_v62(self, current, actual, snapshot)
    }
}

#[cfg(test)]
mod bounds_v62 {
    use super::*;
    #[test]
    fn pr_metadata_members_tab_v62_detail_pages_preserve_complete_empty_and_monotonic_debit() {
        let mut heap = MetadataMembersHeapV62::new(0).unwrap();
        let before = heap.retained_bound();
        assert!(heap.detail_pages(b"[[]]").unwrap().is_empty());
        let first = heap.retained_bound();
        assert!(first >= before + 3 * (512 * 1024 - CHILD_CAP_V51));
        assert!(heap.detail_pages(b"[[]]").unwrap().is_empty());
        let second = heap.retained_bound();
        assert!(second > first);
        assert!(second - first < 3 * (512 * 1024 - CHILD_CAP_V51));
        assert!(heap.detail_pages(b"[]").is_err());
        assert!(heap.detail_pages(b"[{}]").is_err());
    }
    #[test]
    fn pr_metadata_members_tab_v62_detail_decoder_refuses_duplicate_keys_depth_and_trailing() {
        for raw in [
            b"[[{\"id\":1,\"id\":2}]]".to_vec(),
            b"[[]] true".to_vec(),
            format!("{}0{}", "[".repeat(34), "]".repeat(34)).into_bytes(),
        ] {
            assert!(MetadataMembersHeapV62::new(0)
                .unwrap()
                .detail_pages(&raw)
                .is_err());
        }
    }
    #[test]
    fn pr_metadata_members_tab_v62_inherited_model_and_512k_wire_fail_before_next_mapping() {
        let mut heap = MetadataMembersHeapV62::new(MODEL_CAP_V51 - 3 * CHILD_CAP_V51).unwrap();
        assert!(heap.detail_pages(b"[[]]").is_err());
        let oversized = vec![b' '; 512 * 1024 + 1];
        let mut clean = MetadataMembersHeapV62::new(0).unwrap();
        let before = clean.retained_bound();
        assert!(clean.detail_pages(&oversized).is_err());
        assert_eq!(clean.retained_bound(), before);
    }
    #[test]
    fn pr_metadata_members_tab_v62_escaped_keys_and_row_set_debit_precede_clone() {
        let big = format!("[[{{\"{}\":0}}]]", "k".repeat(4097));
        assert!(MetadataMembersHeapV62::new(0)
            .unwrap()
            .detail_pages(big.as_bytes())
            .is_err());
        let raw = br#"[[{"body":"\u96ea\n\"quoted\"","id":1}]]"#;
        let mut heap = MetadataMembersHeapV62::new(0).unwrap();
        let before = heap.retained_bound();
        let rows = heap.detail_pages(raw).unwrap();
        assert_eq!(rows[0]["body"], "雪\n\"quoted\"");
        assert!(
            heap.retained_bound()
                > before
                    + 3 * (512 * 1024 - CHILD_CAP_V51)
                    + 384
                    + 2 * std::mem::size_of::<Value>()
        );
    }
}
