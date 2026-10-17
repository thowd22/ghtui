//! Purpose-specific fresh historical inspection. A Summary descriptor is an
//! observed retained page, never a source mint, Files bypass or write grant.
use super::*;
use crate::providers::gh::{OriginalPrAccessSessionV13, PrResourceParentV1};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

#[derive(PartialEq, Eq)]
struct DatabaseV75 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV75 {
    fn capture(database: &Path) -> Result<Self, ThreadBrowseFailureV1> {
        // Existing OS/private-file policy is mandatory, including for Summary.
        drop(Cache::open(database).map_err(source_error)?);
        let path = std::fs::canonicalize(database).map_err(source_error)?;
        let metadata = std::fs::symlink_metadata(&path).map_err(source_error)?;
        let bytes = path.as_os_str().as_bytes();
        if !metadata.is_file() || bytes.len() > 4096 {
            return Err(ThreadBrowseFailureV1::Source);
        }
        Ok(Self {
            path: bytes.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
enum SessionV75<'a> {
    Read(Box<OriginalPrAccessSessionV13<'a>>),
    Personal(
        Box<crate::personal_pr_entry_v13::summary_history_v75::PersonalSummaryHistoryPhaseV75<'a>>,
    ),
}
#[derive(Clone, Copy)]
enum PhaseOriginV75<'a> {
    Read {
        source: &'a crate::pr_read_origin_v13::PrReadOriginV13,
        active: &'a Arc<ActiveRemoteScopeV1>,
        navigation: &'a Arc<ScopedPrNavigationV1>,
        expected: &'a ThreadHistoryParentObservationV1,
    },
    Personal(&'a Arc<crate::personal_pr_entry_v13::summary_v14::PersonalPrSummaryPresentationV14>),
}
impl PhaseOriginV75<'_> {
    fn scope(&self) -> &Scope {
        match self {
            Self::Read { source, .. } => source.scope(),
            Self::Personal(original) => original.scope(),
        }
    }
}
pub(crate) struct HistoryReadPhaseV75<'a> {
    origin: PhaseOriginV75<'a>,
    session: SessionV75<'a>,
    bounded: Option<GithubProvider>,
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    database: &'a Path,
    database_identity: DatabaseV75,
    cancel: &'a CancellationToken,
    deadline: Instant,
}
impl<'a> HistoryReadPhaseV75<'a> {
    pub(crate) fn begin(
        intent: &'a ThreadHistoryIntentV1,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
    ) -> Result<Self, ThreadBrowseFailureV1> {
        let origin = match &intent.origin {
            HistoryOriginV75::Read {
                source,
                active,
                navigation,
                expected,
            } => PhaseOriginV75::Read {
                source,
                active,
                navigation,
                expected,
            },
            HistoryOriginV75::PersonalSummary(original) => {
                return Self::begin_personal_summary(original, provider, database, cancel, deadline)
            }
        };
        Self::begin_origin(origin, provider, database, cancel, deadline)
    }
    /// Shared read-only Personal Summary phase for purpose-specific inspectors.
    /// The descriptor is genuine; caller still owns its exact UI row and must
    /// supply its own closed marker/origin/reference/body witness at finish.
    pub(crate) fn begin_personal_summary(
        original: &'a Arc<
            crate::personal_pr_entry_v13::summary_v14::PersonalPrSummaryPresentationV14,
        >,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
    ) -> Result<Self, ThreadBrowseFailureV1> {
        Self::begin_origin(
            PhaseOriginV75::Personal(original),
            provider,
            database,
            cancel,
            deadline,
        )
    }
    fn begin_origin(
        origin: PhaseOriginV75<'a>,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
    ) -> Result<Self, ThreadBrowseFailureV1> {
        original_check(cancel, deadline).map_err(source_error)?;
        let (session, bounded, parent, identity) = match origin {
            PhaseOriginV75::Read {
                source,
                navigation,
                expected,
                active,
            } => {
                let mut session = source
                    .begin_access(provider, &Default::default(), cancel)
                    .map_err(source_error)?;
                let identity = session.identity().clone();
                if !source.admits_navigation(active, &identity, navigation, &navigation.canonical) {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                let window = session
                    .reserve_measured_external_reads(2, 128 * 1024, 1024 * 1024, 2, cancel)
                    .map_err(source_error)?;
                let bounded = provider
                    .clone()
                    .with_measured_read_window(window)
                    .map_err(source_error)?;
                let current = session
                    .verify(navigation.original(), cancel)
                    .map_err(source_error)?;
                let parent = bounded
                    .pr_thread_history_parent_v1(current, expected, cancel)
                    .map_err(source_error)?;
                (
                    SessionV75::Read(Box::new(session)),
                    Some(bounded),
                    Arc::new(parent),
                    identity,
                )
            }
            PhaseOriginV75::Personal(original) => {
                let session = crate::personal_pr_entry_v13::summary_history_v75::PersonalSummaryHistoryPhaseV75::begin(original, provider, database, cancel, deadline).map_err(source_error)?;
                let identity = session.identity().clone();
                let parent = session.parent().clone();
                (
                    SessionV75::Personal(Box::new(session)),
                    None,
                    parent,
                    identity,
                )
            }
        };
        if identity.login.len() > 4096
            || Scope::github(&identity.host, identity.account_id)
                .ok()
                .as_ref()
                != Some(origin.scope())
        {
            return Err(ThreadBrowseFailureV1::Source);
        }
        let phase = Self {
            origin,
            session,
            bounded,
            parent,
            identity,
            database,
            database_identity: DatabaseV75::capture(database)?,
            cancel,
            deadline,
        };
        phase.validate_database()?;
        Ok(phase)
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    fn validate_database(&self) -> Result<(), ThreadBrowseFailureV1> {
        original_check(self.cancel, self.deadline).map_err(source_error)?;
        if DatabaseV75::capture(self.database)? != self.database_identity {
            return Err(ThreadBrowseFailureV1::Source);
        }
        match self.origin {
            PhaseOriginV75::Read { source, .. } => source
                .validate_cache(&Cache::open(self.database).map_err(source_error)?)
                .map_err(source_error),
            PhaseOriginV75::Personal(original) => {
                if DatabaseV75::capture(original.database())? != self.database_identity {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                Ok(())
            }
        }
    }
    /// All payload/labels/Doc preparation precedes this method. The callback
    /// rechecks the exact historical marker/origin/reference/body snapshot,
    /// never a generic write guard. Final actual UID is the last private gate.
    pub(crate) fn finish(
        self,
        guard: impl FnOnce() -> crate::providers::gh::GithubResult<()>,
    ) -> Result<HostIdentity, ThreadBrowseFailureV1> {
        match self {
            Self {
                session: SessionV75::Personal(phase),
                ..
            } => phase.finish(guard).map_err(source_error),
            read => read.finish_read(guard),
        }
    }
    fn finish_read(
        self,
        guard: impl FnOnce() -> crate::providers::gh::GithubResult<()>,
    ) -> Result<HostIdentity, ThreadBrowseFailureV1> {
        self.validate_database()?;
        let last = match (&self.session, self.origin) {
            (SessionV75::Read(session), PhaseOriginV75::Read { expected, .. }) => {
                let current = session
                    .observation_v1(self.parent.repository_id())
                    .ok_or(ThreadBrowseFailureV1::Source)?;
                self.bounded
                    .as_ref()
                    .ok_or(ThreadBrowseFailureV1::Source)?
                    .pr_thread_history_parent_v1(current, expected, self.cancel)
                    .map_err(source_error)?
            }
            _ => return Err(ThreadBrowseFailureV1::Source),
        };
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, &last) {
            return Err(ThreadBrowseFailureV1::Source);
        }
        // Move only the live session. The source/DB witnesses remain borrowed
        // by the single original SQL closure; neither pool nor clock is renewed.
        let Self {
            mut session,
            origin,
            database,
            database_identity,
            cancel,
            deadline,
            ..
        } = self;
        let local = || {
            original_check(cancel, deadline)?;
            let validate = || -> Result<(), ThreadBrowseFailureV1> {
                if DatabaseV75::capture(database)? != database_identity {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                if let PhaseOriginV75::Read { source, .. } = origin {
                    source
                        .validate_cache(&Cache::open(database).map_err(source_error)?)
                        .map_err(source_error)?;
                }
                if let PhaseOriginV75::Personal(original) = origin {
                    if DatabaseV75::capture(original.database())? != database_identity {
                        return Err(ThreadBrowseFailureV1::Source);
                    }
                }
                Ok(())
            };
            validate().map_err(|_| safe_local())?;
            guard()?;
            validate().map_err(|_| safe_local())?;
            original_check(cancel, deadline)
        };
        let identity = match &mut session {
            SessionV75::Read(read) => {
                let PhaseOriginV75::Read { navigation, .. } = origin else {
                    return Err(ThreadBrowseFailureV1::Source);
                };
                read.verify(navigation.original(), cancel)
                    .map_err(source_error)?;
                let SessionV75::Read(read) = session else {
                    unreachable!()
                };
                read.close_with_guard(cancel, local)
                    .map_err(source_error)?
                    .identity()
                    .clone()
            }
            SessionV75::Personal(_) => return Err(ThreadBrowseFailureV1::Source),
        };
        original_check(cancel, deadline).map_err(source_error)?;
        if identity.login.len() > 4096
            || Scope::github(&identity.host, identity.account_id)
                .ok()
                .as_ref()
                != Some(origin.scope())
        {
            return Err(ThreadBrowseFailureV1::Source);
        }
        Ok(identity)
    }
}
