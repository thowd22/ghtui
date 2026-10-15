//! Genuine full-file context, not patch coordinates or selected-change authority.
//! Projection reuses the native diff matcher, escaping, lexer and wrapping.
use crate::{
    git::{MutableFullSideV3, OriginalComparisonSideV1, OriginalComparisonSourceV1},
    identity::{ActorIdentity, ActorKind, GitRole},
    native_diff_operation_v3::NativeDiffCheckV3,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_native_diff_content_v2::NativeDiffPurposeV2,
    repository_native_diff_projection_v1::{
        paint, text_chunks_v3, visit_line_matches_v3, NativeDiffOptionsV1, NativeDiffPaintRoleV1,
        NativeDiffQueryV1, NativeDiffSpanV1, NativeDiffWhitespaceV1,
    },
    repository_view::{
        content_preview_rows_v3, preview_line_ranges_v3, BlobPreviewKind, DiffSide,
        FilePreviewSourceV3,
    },
    workspace::PreparedOriginalDiffSideV1,
};
use std::{ops::Range, sync::Arc};

#[derive(Debug)]
pub struct NativeFullSideMaterialV3 {
    value: MaterialV3,
}
#[derive(Debug)]
enum MaterialV3 {
    RemotePr(Arc<crate::scoped_pr_native_diff_source_v1::ScopedPrNativeSideV1>),
    Saved(Arc<PreparedOriginalDiffSideV1>),
    Mutable(Arc<MutableFullSideV3>),
    Absent(Arc<str>),
}
impl NativeFullSideMaterialV3 {
    pub(crate) fn remote_pr_v1(
        side: Arc<crate::scoped_pr_native_diff_source_v1::ScopedPrNativeSideV1>,
    ) -> Arc<Self> {
        Arc::new(Self {
            value: MaterialV3::RemotePr(side),
        })
    }
    pub(crate) fn is_original_pr_v1(
        &self,
        proof: &Arc<
            crate::scoped_pr_observed_tabs_v1::resources::immutable_images::ClosedPrFileImagesV1,
        >,
    ) -> bool {
        matches!(&self.value, MaterialV3::RemotePr(side) if Arc::ptr_eq(&side.proof, proof))
    }
    pub(crate) fn mutable(side: Arc<MutableFullSideV3>) -> Arc<Self> {
        Arc::new(Self {
            value: MaterialV3::Mutable(side),
        })
    }
    pub(crate) fn saved(
        source: Arc<OriginalComparisonSourceV1>,
        file: usize,
        side: DiffSide,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let side = match side {
            DiffSide::Old => OriginalComparisonSideV1::Old,
            DiffSide::New => OriginalComparisonSideV1::New,
            DiffSide::Both => return Err("Choose explicit Old or New full side".into()),
        };
        let frame = crate::repository_diff_sides_v1::OriginalDiffFrameV1::original_route_frame_v1(
            source, file,
        )?;
        let ticket = frame
            .ticket(0, side)
            .ok_or("Original full-side ticket unavailable")?;
        let value = match ticket.absent_reason() {
            Some(reason) => MaterialV3::Absent(Arc::from(reason)),
            None => MaterialV3::Saved(PreparedOriginalDiffSideV1::open(&ticket, operation)?),
        };
        let material = Arc::new(Self { value });
        operation.check()?;
        Ok(material)
    }
    pub(crate) fn content(&self) -> &[u8] {
        match &self.value {
            MaterialV3::RemotePr(side) => side.content(),
            MaterialV3::Saved(side) => &side.preview().content,
            MaterialV3::Mutable(side) => side.content(),
            MaterialV3::Absent(_) => &[],
        }
    }
    pub(crate) fn kind(&self) -> Option<&BlobPreviewKind> {
        match &self.value {
            MaterialV3::RemotePr(side) => side.kind.as_ref(),
            MaterialV3::Saved(side) => Some(&side.preview().kind),
            MaterialV3::Mutable(side) if !side.absent() => Some(side.kind()),
            _ => None,
        }
    }
    pub(crate) fn label(&self) -> &str {
        match &self.value {
            MaterialV3::RemotePr(side) => &side.label,
            MaterialV3::Saved(side) => side.sha(),
            MaterialV3::Mutable(side) => side.label(),
            MaterialV3::Absent(reason) => reason,
        }
    }
    fn preview_source(&self) -> FilePreviewSourceV3<'_> {
        match &self.value {
            MaterialV3::RemotePr(side) => FilePreviewSourceV3::Saved(side.object()),
            MaterialV3::Saved(side) => FilePreviewSourceV3::Saved(&side.entry().id),
            MaterialV3::Mutable(side) if side.raw_worktree() => FilePreviewSourceV3::RawWorktree,
            MaterialV3::Mutable(side) if side.observed_head() => {
                FilePreviewSourceV3::ObservedHead(side.object().unwrap_or("unavailable"))
            }
            MaterialV3::Mutable(side) => {
                FilePreviewSourceV3::Index(side.object().unwrap_or("unavailable"))
            }
            MaterialV3::Absent(_) => FilePreviewSourceV3::RawWorktree,
        }
    }
    pub(crate) fn validate_local_v3(
        &self,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<(), String> {
        match &self.value {
            MaterialV3::Mutable(side) => side.validate_current(operation),
            _ => operation.check(),
        }
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        if let MaterialV3::RemotePr(side) = &self.value {
            return side.retained_bytes() + std::mem::size_of::<Self>() + 64;
        }
        if let MaterialV3::Mutable(side) = &self.value {
            return side.retained_bytes() + std::mem::size_of::<Self>() + 64;
        }
        let saved = match &self.value {
            MaterialV3::Saved(side) => {
                side.preview()
                    .rows
                    .iter()
                    .map(|row| row.label.capacity() + std::mem::size_of_val(row))
                    .sum::<usize>()
                    + side
                        .rows()
                        .iter()
                        .map(|row| row.label.capacity() + std::mem::size_of_val(row))
                        .sum::<usize>()
                    + commit_bytes_v3(side.commit_v1())
                    + side
                        .preview()
                        .rows
                        .capacity()
                        .saturating_sub(side.preview().rows.len())
                        * std::mem::size_of::<crate::repository_view::FilePreviewLine>()
                    + 512
            }
            _ => 0,
        };
        let content_capacity = match &self.value {
            MaterialV3::Saved(side) => side.preview().content.capacity(),
            _ => 0,
        };
        content_capacity + self.label().len() + 512 + saved
    }
    pub(crate) fn external_new_material_bytes_v4(&self) -> usize {
        if let MaterialV3::RemotePr(side) = &self.value {
            // Both immutable images/proof already belong to this source. Only
            // the newly allocated side adapter/label is new preparation here.
            return std::mem::size_of::<Self>()
                + std::mem::size_of_val(side.as_ref())
                + side.label.capacity()
                + 128;
        }
        self.retained_bytes()
    }
    fn actors(&self) -> (ActorIdentity, ActorIdentity) {
        match &self.value {
            MaterialV3::RemotePr(side) => (
                ActorIdentity::github(
                    &side.proof.source_scope().host,
                    &side.proof.observed().parent().summary().pr.author,
                ),
                ActorIdentity {
                    host: None,
                    login: None,
                    name: "Commit attribution not observed".into(),
                    email: None,
                    avatar_url: None,
                    kind: ActorKind::Unknown,
                    git_role: None,
                    host_associated: false,
                },
            ),
            MaterialV3::Saved(side) => {
                let commit = side.commit_v1();
                (
                    crate::git_actor_labels_v1::display_identity(&commit.author, GitRole::Author),
                    crate::git_actor_labels_v1::display_identity(
                        &commit.committer,
                        GitRole::Committer,
                    ),
                )
            }
            _ => {
                let unknown = |role| ActorIdentity {
                    kind: ActorKind::Unknown,
                    host: None,
                    login: None,
                    name: "unknown • no full-side actor association observed".into(),
                    email: None,
                    avatar_url: None,
                    git_role: Some(role),
                    host_associated: false,
                };
                (unknown(GitRole::Author), unknown(GitRole::Committer))
            }
        }
    }
}

pub struct NativeFullSideRowV3 {
    text: Arc<str>,
    spans: Arc<[NativeDiffSpanV1]>,
    line: Option<usize>,
    continuation: bool,
}
impl NativeFullSideRowV3 {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn spans(&self) -> &[NativeDiffSpanV1] {
        &self.spans
    }
    pub fn line(&self) -> Option<usize> {
        self.line
    }
    pub fn continuation(&self) -> bool {
        self.continuation
    }
}
/// Ticket permits ONLY pointing at a current read-only display row. No public
/// constructor, canonical diff key, range, plan or mutation conversion exists.
pub struct NativeFullSideTicketV3<S: NativeDiffPurposeV2> {
    source: Arc<S>,
    epoch: Arc<()>,
    row: usize,
}
impl<S: NativeDiffPurposeV2> Clone for NativeFullSideTicketV3<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            epoch: self.epoch.clone(),
            row: self.row,
        }
    }
}
impl<S: NativeDiffPurposeV2> PartialEq for NativeFullSideTicketV3<S> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
            && Arc::ptr_eq(&self.epoch, &other.epoch)
            && self.row == other.row
    }
}
impl<S: NativeDiffPurposeV2> Eq for NativeFullSideTicketV3<S> {}

pub struct NativeFullSideV3<S: NativeDiffPurposeV2> {
    source: Arc<S>,
    file: usize,
    side: DiffSide,
    material: Arc<NativeFullSideMaterialV3>,
    epoch: Arc<()>,
    options: NativeDiffOptionsV1,
    query: NativeDiffQueryV1,
    width: u16,
    rows: Arc<[NativeFullSideRowV3]>,
    matches: Arc<[usize]>,
    matches_capped: bool,
    truncated: bool,
    syntax: &'static str,
    plain: usize,
    bytes: usize,
    author: ActorIdentity,
    committer: ActorIdentity,
    author_label: Arc<str>,
    committer_label: Arc<str>,
    path_label: Arc<str>,
}
impl<S: NativeDiffPurposeV2> NativeFullSideV3<S> {
    pub(crate) fn prepare(
        source: Arc<S>,
        (file, side): (usize, DiffSide),
        retained: Option<&Arc<Self>>,
        options: NativeDiffOptionsV1,
        query: NativeDiffQueryV1,
        width: u16,
        operation: &S::Operation,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if !(12..=512).contains(&width) || side == DiffSide::Both || file >= source.member_count() {
            return Err(
                "Full-side request requires genuine bounded member, width and explicit side".into(),
            );
        }
        let material = if let Some(old) = retained
            .filter(|old| Arc::ptr_eq(&old.source, &source) && old.file == file && old.side == side)
        {
            source.validate_full_side_v3(&old.material, operation)?;
            old.material.clone()
        } else {
            source.prepare_full_side_v3(file, side, operation)?
        };
        let content = material.content();
        if content.len() > 2 * 1024 * 1024 {
            return Err("Full-side raw content exceeds2MiB".into());
        }
        let regex = query.compile()?;
        let path_label: Arc<str> = Arc::from(
            source
                .member_path(file, side)
                .map(|path| crate::git::display_path_bytes(&path.0))
                .unwrap_or_else(|| "ABSENT • no raw path".into()),
        );
        let (author, committer) = material.actors();
        let (author_label, committer_label): (Arc<str>, Arc<str>) =
            if matches!(&material.value, MaterialV3::RemotePr(_)) {
                (
                    Arc::from(format!(
                        "PR author: {} • NOT file-line attribution",
                        author.label()
                    )),
                    Arc::from("Commit author/committer not observed"),
                )
            } else {
                (Arc::from(author.label()), Arc::from(committer.label()))
            };
        let language =
            paint::Language::for_path(source.member_path(file, side).map(|path| path.0.as_slice()));
        let mut lexer = paint::Lexer::default();
        let mut rows = vec![];
        let mut matches = vec![];
        let mut occurrences = 0;
        let mut capped = false;
        let mut plain = 0;
        let mut bytes = material.retained_bytes()
            + std::mem::size_of::<Self>()
            + query.text().len()
            + author_label.len() * 2
            + committer_label.len() * 2
            + path_label.len()
            + 64
            + 1024;
        let mut truncated = false;
        if material.kind() == Some(&BlobPreviewKind::Text) {
            let mut estimate = bytes;
            for (index, ch) in std::str::from_utf8(content)
                .map_err(|_| "Text classifier contradicted full-side UTF8")?
                .chars()
                .enumerate()
            {
                if index % 256 == 0 {
                    operation.check()?;
                }
                estimate = estimate.saturating_add(match ch {
                    '\n' | '\r' | '\t' => 2,
                    ch if ch.is_control()
                        || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') =>
                    {
                        4 + ((32 - (ch as u32).leading_zeros()).max(1) as usize).div_ceil(4)
                    }
                    ch => ch.len_utf8(),
                });
                if estimate > 8 * 1024 * 1024 {
                    return Err("Full-side escaped display exceeds8MiB before allocation".into());
                }
            }
            let (lines, partial) = preview_line_ranges_v3(content, 10_000);
            truncated = partial;
            for (index, range) in lines.into_iter().enumerate() {
                operation.check()?;
                let raw = &content[range];
                let fallback = raw.len() > 4096;
                let mut spans = if options.syntax && !fallback {
                    lexer.spans(raw, language, operation)?
                } else {
                    if fallback {
                        plain += 1;
                    }
                    vec![]
                };
                let mut matched = false;
                let complete = visit_line_matches_v3(
                    &query,
                    regex.as_ref(),
                    raw,
                    operation,
                    |range: Range<usize>| {
                        if occurrences >= 10_000 {
                            capped = true;
                            return false;
                        }
                        occurrences += 1;
                        spans.push((range, NativeDiffPaintRoleV1::Search));
                        matched = true;
                        true
                    },
                )?;
                if !complete {
                    capped = true;
                }
                let (text, spans) = paint::escape(
                    raw,
                    &spans,
                    options.whitespace == NativeDiffWhitespaceV1::Mark,
                    operation,
                )?;
                let first = rows.len();
                let chunks = if options.wrap && !fallback {
                    text_chunks_v3(
                        &text,
                        usize::from(width.saturating_sub(9)).max(1),
                        operation,
                    )?
                } else {
                    std::iter::once(0..text.len()).collect()
                };
                for (part, chunk) in chunks.into_iter().enumerate() {
                    let clipped = spans
                        .iter()
                        .filter_map(|span| {
                            let start = span.bytes.start.max(chunk.start);
                            let end = span.bytes.end.min(chunk.end);
                            (start < end).then(|| NativeDiffSpanV1 {
                                bytes: start - chunk.start..end - chunk.start,
                                role: span.role,
                            })
                        })
                        .collect::<Vec<_>>();
                    bytes = bytes.saturating_add(
                        chunk.len()
                            + clipped.len() * std::mem::size_of::<NativeDiffSpanV1>()
                            + std::mem::size_of::<NativeFullSideRowV3>()
                            + 64,
                    );
                    if bytes > 8 * 1024 * 1024 || rows.len() == 50_000 {
                        return Err("Full-side display exceeds8MiB/50k bounded prepared rows; no row authority".into());
                    }
                    rows.push(NativeFullSideRowV3 {
                        text: Arc::from(&text[chunk]),
                        spans: clipped.into(),
                        line: Some(index + 1),
                        continuation: part != 0,
                    });
                }
                if matched {
                    matches.push(first);
                }
            }
        } else {
            let labels = match material.kind() {
                Some(kind) => {
                    content_preview_rows_v3(kind, content, 10_000, material.preview_source())
                        .0
                        .into_iter()
                        .map(|row| row.label)
                        .collect::<Vec<_>>()
                }
                None => vec![material.label().into()],
            };
            for text in labels {
                bytes += text.len() + 128;
                rows.push(NativeFullSideRowV3 {
                    text: Arc::from(text),
                    spans: Arc::from([]),
                    line: None,
                    continuation: false,
                });
            }
        }
        bytes = bytes.saturating_add(matches.len() * std::mem::size_of::<usize>() + 128);
        if bytes > 8 * 1024 * 1024 {
            return Err("Full-side combined prepared material exceeds8MiB".into());
        }
        source.validate_full_side_v3(&material, operation)?;
        source.validate_current(operation)?;
        let syntax = if !options.syntax {
            "Syntax disabled • plain full-file text"
        } else if plain > 0 {
            "Shared bounded lexer • long lines plain; full-file prefix"
        } else {
            language.full_side_label()
        };
        let frame = Arc::new(Self {
            source,
            file,
            side,
            material,
            epoch: Arc::new(()),
            options,
            query,
            width,
            rows: rows.into(),
            matches: matches.into(),
            matches_capped: capped,
            truncated,
            syntax,
            plain,
            bytes,
            author,
            committer,
            author_label,
            committer_label,
            path_label,
        });
        operation.check()?;
        Ok(frame)
    }
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn file(&self) -> usize {
        self.file
    }
    pub fn side(&self) -> DiffSide {
        self.side
    }
    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn query(&self) -> &NativeDiffQueryV1 {
        &self.query
    }
    pub fn options(&self) -> &NativeDiffOptionsV1 {
        &self.options
    }
    pub fn rows(&self) -> &[NativeFullSideRowV3] {
        &self.rows
    }
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }
    pub fn matches_capped(&self) -> bool {
        self.matches_capped
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
    pub fn label(&self) -> &str {
        self.material.label()
    }
    pub fn path_label(&self) -> &str {
        &self.path_label
    }
    pub fn syntax(&self) -> &str {
        self.syntax
    }
    pub fn plain(&self) -> usize {
        self.plain
    }
    pub fn coverage(&self) -> &'static str {
        match self.material.kind() {
            None => "ABSENT • no file body or text coordinates",
            Some(BlobPreviewKind::Text) if self.truncated => {
                "PARTIAL first10k lines • later context not rendered"
            }
            Some(BlobPreviewKind::Text) if self.plain > 0 => {
                "Bytes loaded • long-line horizontal view capped4096"
            }
            Some(BlobPreviewKind::Text) => "Complete text within2MiB • read-only context",
            Some(BlobPreviewKind::Binary) => "Binary metadata only • body not rendered",
            Some(BlobPreviewKind::LfsPointer { .. }) => {
                "LFS pointer only • large content NOT fetched"
            }
            Some(BlobPreviewKind::Symlink) => "Literal symlink only • target NOT followed",
            Some(BlobPreviewKind::Submodule) => "Gitlink metadata only • nested files NOT loaded",
        }
    }
    pub fn prepared_bytes(&self) -> usize {
        self.bytes
    }
    pub fn author(&self) -> (&ActorIdentity, &str) {
        (&self.author, &self.author_label)
    }
    pub fn committer(&self) -> (&ActorIdentity, &str) {
        (&self.committer, &self.committer_label)
    }
    pub fn ticket(&self, row: usize) -> Option<NativeFullSideTicketV3<S>> {
        self.rows.get(row)?;
        Some(NativeFullSideTicketV3 {
            source: self.source.clone(),
            epoch: self.epoch.clone(),
            row,
        })
    }
    pub fn accepts(&self, ticket: &NativeFullSideTicketV3<S>, row: usize) -> bool {
        row < self.rows.len()
            && ticket.row == row
            && Arc::ptr_eq(&ticket.source, &self.source)
            && Arc::ptr_eq(&ticket.epoch, &self.epoch)
    }
}
fn commit_bytes_v3(commit: &crate::git::Commit) -> usize {
    std::mem::size_of_val(commit)
        + commit.id.capacity()
        + commit.subject.capacity()
        + commit.parents.capacity() * std::mem::size_of::<String>()
        + commit.parents.iter().map(String::capacity).sum::<usize>()
        + commit.refs.capacity() * std::mem::size_of::<String>()
        + commit.refs.iter().map(String::capacity).sum::<usize>()
        + commit.raw_refs.capacity() * std::mem::size_of::<Vec<u8>>()
        + commit.raw_refs.iter().map(Vec::capacity).sum::<usize>()
        + [&commit.author, &commit.committer]
            .iter()
            .map(|actor| actor.name.capacity() + actor.email.capacity())
            .sum::<usize>()
        + 128
}
#[cfg(test)]
#[path = "repository_native_full_side_v3_tests.rs"]
mod tests;
