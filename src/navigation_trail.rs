//! Ephemeral, bounded navigation intent. This is not GitHub access evidence,
//! a publication permit, a cached workspace, or serialized action authority.
//!
//! The application supplies exact keys from its typed prepared models. A
//! bookmark never parses displayed breadcrumbs or stores a numeric row index.
//! Restoring a missing source must use a bounded exact read or an explanation,
//! never a broader/default scope. Local native Back/crumb journeys are verified;
//! private/nested source adapters and wider PTY coverage remain open.
use crate::app::View;
use std::{collections::VecDeque, fmt, os::unix::ffi::OsStrExt, path::PathBuf, sync::Arc};

const MAX_HISTORY: usize = 64;
const MAX_BOOKMARK_BYTES: usize = 16 * 1024;
const MAX_KEY_BYTES: usize = 4096;
const MAX_REPOSITORIES: usize = 128;
const MAX_SCROLL: usize = 1_000_000;

#[derive(Clone, PartialEq, Eq)]
pub enum NavigationScope {
    /// Public setup/empty view only; never private data under an unknown caller.
    Shell,
    /// Preserve exact case and raw OS bytes. Construction performs no I/O.
    Local(PathBuf),
    /// Numeric caller namespace, not proof that the caller was freshly read.
    GitHub { host: String, account_id: u64 },
}
impl fmt::Debug for NavigationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Shell => "Shell",
            Self::Local(_) => "Local",
            Self::GitHub { .. } => "GitHub",
        })
    }
}
impl NavigationScope {
    pub fn local(directory: PathBuf) -> Result<Self, String> {
        if !directory.is_absolute() || directory.as_os_str().as_bytes().len() > MAX_KEY_BYTES {
            return Err(
                "Navigation needs the original bounded absolute repository directory".into(),
            );
        }
        Ok(Self::Local(directory))
    }
    pub fn github(host: &str, account_id: u64) -> Result<Self, String> {
        if account_id == 0 {
            return Err(
                "Navigation cannot restore private pages without a positive numeric caller".into(),
            );
        }
        Ok(Self::GitHub {
            host: canonical_host(host)?,
            account_id,
        })
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Shell => 0,
            Self::Local(path) => path.as_os_str().as_bytes().len(),
            Self::GitHub { host, .. } => host.len() + 8,
        }
    }
    fn allowed(&self, identity: Option<&Self>) -> bool {
        !matches!(self, Self::GitHub { .. }) || identity == Some(self)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationPosition {
    /// Genuine prepared row identity (raw Git path, exact resource/commit key),
    /// not its old displayed index. The restore adapter must look it up again.
    pub selected_key: Option<Vec<u8>>,
    pub focused_control: Option<String>,
    pub content_focused: bool,
    pub scroll: usize,
    pub graph_v2: Option<crate::repository_graph_v1::GraphPositionV2>,
}
impl NavigationPosition {
    fn validate(&self) -> Result<usize, String> {
        if self.scroll > MAX_SCROLL
            || self
                .selected_key
                .as_ref()
                .is_some_and(|key| key.is_empty() || key.len() > MAX_KEY_BYTES)
            || self
                .focused_control
                .as_ref()
                .is_some_and(|key| !safe_id(key))
        {
            return Err("Navigation position exceeds its bound or lacks a stable identity".into());
        }
        let graph_bytes = self
            .graph_v2
            .as_ref()
            .map_or(Ok(0), |position| position.navigation_metadata_bytes_v2())?;
        Ok(graph_bytes
            + self.selected_key.as_ref().map_or(0, Vec::len)
            + self.focused_control.as_ref().map_or(0, String::len)
            + 16)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct NavigationBookmark {
    view: View,
    scope: NavigationScope,
    /// Exact prepared source/revision identity. Bytes retain non-UTF8 paths.
    source_key: Vec<u8>,
    /// Closed caller-owned page/subview ID; never a command or publication.
    route_key: String,
    repositories: Vec<String>,
    label: String,
    position: NavigationPosition,
}
impl fmt::Debug for NavigationBookmark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NavigationBookmark")
            .field("view", &self.view)
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}
impl NavigationBookmark {
    pub fn new(
        view: View,
        scope: NavigationScope,
        source_key: Vec<u8>,
        route_key: String,
        repositories: Vec<String>,
        label: String,
        position: NavigationPosition,
    ) -> Result<Self, String> {
        if source_key.is_empty()
            || source_key.len() > MAX_KEY_BYTES
            || !safe_id(&route_key)
            || repositories.len() > MAX_REPOSITORIES
            || label.is_empty()
            || label.len() > 1024
            || !safe_line(&label)
        {
            return Err(
                "Navigation bookmark needs bounded exact source/route and safe readable text"
                    .into(),
            );
        }
        // Validate even a directly constructed scope before retaining it.
        let scope = match scope {
            NavigationScope::Shell => NavigationScope::Shell,
            NavigationScope::Local(path) => NavigationScope::local(path)?,
            NavigationScope::GitHub { host, account_id } => {
                NavigationScope::github(&host, account_id)?
            }
        };
        if !matches!(scope, NavigationScope::GitHub { .. }) && !repositories.is_empty() {
            return Err(
                "Local/public navigation cannot carry private repository membership".into(),
            );
        }
        let mut normalized = Vec::with_capacity(repositories.len());
        let mut bytes = scope.bytes()
            + source_key.len()
            + route_key.len()
            + label.len()
            + position.validate()?;
        for repository in repositories {
            bytes = bytes
                .checked_add(repository.len())
                .ok_or("Navigation bookmark byte accounting overflow")?;
            if bytes > MAX_BOOKMARK_BYTES {
                return Err("Navigation bookmark exceeds 16 KiB".into());
            }
            normalized.push(canonical_repository(&repository)?);
        }
        if bytes > MAX_BOOKMARK_BYTES {
            return Err("Navigation bookmark exceeds 16 KiB".into());
        }
        normalized.sort();
        normalized.dedup();
        Ok(Self {
            view,
            scope,
            source_key,
            route_key,
            repositories: normalized,
            label,
            position,
        })
    }
    pub fn view(&self) -> View {
        self.view
    }
    pub fn scope(&self) -> &NavigationScope {
        &self.scope
    }
    pub fn source_key(&self) -> &[u8] {
        &self.source_key
    }
    pub fn route_key(&self) -> &str {
        &self.route_key
    }
    pub fn repositories(&self) -> &[String] {
        &self.repositories
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn position(&self) -> &NavigationPosition {
        &self.position
    }
    pub fn same_location(&self, other: &Self) -> bool {
        self.view == other.view
            && self.scope == other.scope
            && self.source_key == other.source_key
            && self.route_key == other.route_key
            && self.repositories == other.repositories
    }
    /// A position is not valid for a newer source merely because the view or
    /// repository name looks the same. No numeric row fallback is returned.
    pub fn matching_position(&self, current: &Self) -> Option<&NavigationPosition> {
        self.same_location(current).then_some(&self.position)
    }
    fn affected_repository(&self, scope: &NavigationScope, repository: &str) -> bool {
        &self.scope == scope
            && (self.repositories.is_empty()
                || self.repositories.iter().any(|name| name == repository))
    }
}

/// Owner/epoch-bound link supplied with an observed breadcrumb. A renderer may
/// clone it for shared keyboard/mouse hits; an old layout cannot retarget it.
#[derive(Clone)]
pub struct NavigationLink {
    owner: Arc<()>,
    epoch: u64,
    entry_id: u64,
}
impl fmt::Debug for NavigationLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NavigationLink").finish_non_exhaustive()
    }
}
impl PartialEq for NavigationLink {
    fn eq(&self, other: &Self) -> bool {
        // Arc<()> value equality would equate unrelated owners. UI action
        // identity must retain actual producer ownership, not just scalar IDs.
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.epoch == other.epoch
            && self.entry_id == other.entry_id
    }
}
impl Eq for NavigationLink {}
/// Closed shared keyboard/mouse intent, never a parsed displayed breadcrumb,
/// numeric history slot, arbitrary executable or restored publication permit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationAction {
    Back,
    Breadcrumb(NavigationLink),
}
pub struct NavigationCrumb<'a> {
    pub label: &'a str,
    pub view: View,
    pub link: NavigationLink,
}
/// Single-use navigation bookkeeping only. No Clone/serde/public constructor.
pub struct NavigationToken {
    owner: Arc<()>,
    epoch: u64,
}
impl fmt::Debug for NavigationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NavigationToken").finish_non_exhaustive()
    }
}
struct Entry {
    id: u64,
    bookmark: NavigationBookmark,
}
enum Direction {
    Forward,
    Restore(u64),
}
struct Pending {
    epoch: u64,
    destination: NavigationBookmark,
    direction: Direction,
}
pub struct NavigationTrail {
    owner: Arc<()>,
    epoch: u64,
    next_entry: u64,
    identity: Option<NavigationScope>,
    current: Option<Entry>,
    history: VecDeque<Entry>,
    pending: Option<Pending>,
}
impl Default for NavigationTrail {
    fn default() -> Self {
        Self {
            owner: Arc::new(()),
            epoch: 0,
            next_entry: 0,
            identity: None,
            current: None,
            history: VecDeque::new(),
            pending: None,
        }
    }
}
impl fmt::Debug for NavigationTrail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NavigationTrail")
            .field("history", &self.history.len())
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}
impl NavigationTrail {
    pub fn current(&self) -> Option<&NavigationBookmark> {
        self.current.as_ref().map(|entry| &entry.bookmark)
    }
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Seed/refresh the observed page without manufacturing a navigation event.
    /// External code must separately guard the actual source and clear UI hits.
    pub fn observe_current(&mut self, bookmark: NavigationBookmark) -> Result<(), String> {
        self.ensure_allowed(&bookmark)?;
        let next_id = self
            .next_entry
            .checked_add(1)
            .ok_or("Navigation identity space exhausted")?;
        self.advance()?;
        self.next_entry = next_id;
        self.current = Some(Entry {
            id: next_id,
            bookmark,
        });
        self.pending = None;
        Ok(())
    }
    pub fn update_position(&mut self, observed: &NavigationBookmark) -> Result<(), String> {
        self.ensure_allowed(observed)?;
        let current = self.current.as_mut().ok_or("No current navigation page")?;
        if !current.bookmark.same_location(observed) {
            return Err(
                "Current navigation source changed; old selection is not transferable".into(),
            );
        }
        current.bookmark.position = observed.position.clone();
        Ok(())
    }
    pub fn begin(&mut self, destination: NavigationBookmark) -> Result<NavigationToken, String> {
        self.stage(destination, Direction::Forward)
    }
    pub fn back(&mut self) -> Result<NavigationToken, String> {
        let entry = self
            .history
            .back()
            .ok_or("No earlier navigation page is available")?;
        self.stage(entry.bookmark.clone(), Direction::Restore(entry.id))
    }
    pub fn jump(&mut self, link: &NavigationLink) -> Result<NavigationToken, String> {
        if !Arc::ptr_eq(&self.owner, &link.owner) || link.epoch != self.epoch {
            return Err(
                "Navigation breadcrumb changed; choose the current visible destination".into(),
            );
        }
        let entry = self
            .history
            .iter()
            .find(|entry| entry.id == link.entry_id)
            .ok_or("Original navigation destination is no longer retained")?;
        self.stage(entry.bookmark.clone(), Direction::Restore(entry.id))
    }
    pub fn action(&mut self, action: NavigationAction) -> Result<NavigationToken, String> {
        match action {
            NavigationAction::Back => self.back(),
            NavigationAction::Breadcrumb(link) => self.jump(&link),
        }
    }
    pub fn request(&self, token: &NavigationToken) -> Result<&NavigationBookmark, String> {
        self.check(token).map(|pending| &pending.destination)
    }
    pub fn complete(
        &mut self,
        token: NavigationToken,
        observed: NavigationBookmark,
    ) -> Result<(), String> {
        let pending = self.check(&token)?;
        let matches = pending.destination.same_location(&observed)
            && observed.scope.allowed(self.identity.as_ref());
        let next_id = self
            .next_entry
            .checked_add(1)
            .ok_or("Navigation identity space exhausted")?;
        self.advance()?;
        let pending = self
            .pending
            .take()
            .expect("checked current navigation transaction");
        if !matches {
            return Err("Navigation opened a changed source; original history is preserved without retargeting".into());
        }
        match pending.direction {
            Direction::Forward => {
                if let Some(previous) = self.current.take() {
                    if !previous.bookmark.same_location(&observed) {
                        self.history.push_back(previous);
                    }
                }
                while self.history.len() > MAX_HISTORY {
                    self.history.pop_front();
                }
            }
            Direction::Restore(id) => {
                let index = self
                    .history
                    .iter()
                    .position(|entry| entry.id == id)
                    .ok_or("Original history changed during navigation")?;
                // Successful Back/jump removes that destination and every later
                // page. Failed reads/cancel never take this branch or pop history.
                self.history.truncate(index);
            }
        }
        self.next_entry = next_id;
        self.current = Some(Entry {
            id: next_id,
            bookmark: observed,
        });
        Ok(())
    }
    pub fn cancel(&mut self, token: NavigationToken) -> Result<(), String> {
        self.check(&token)?;
        self.advance()?;
        self.pending = None;
        Ok(())
    }
    /// The application owner may supersede an in-flight read even when its
    /// single-use token has moved into the worker. History is never popped.
    pub fn abandon(&mut self) -> Result<(), String> {
        self.advance()?;
        self.pending = None;
        Ok(())
    }
    pub fn crumbs(&self) -> Vec<NavigationCrumb<'_>> {
        if self.pending.is_some() {
            return vec![];
        }
        self.history
            .iter()
            .map(|entry| NavigationCrumb {
                label: entry.bookmark.label(),
                view: entry.bookmark.view(),
                link: NavigationLink {
                    owner: Arc::clone(&self.owner),
                    epoch: self.epoch,
                    entry_id: entry.id,
                },
            })
            .collect()
    }
    /// Called only with the application's actual fresh principal result. This
    /// method validates namespace format, not authentication or repository access.
    pub fn remote_identity(&mut self, identity: Option<NavigationScope>) -> Result<(), String> {
        let identity = match identity {
            Some(NavigationScope::GitHub { host, account_id }) => {
                match NavigationScope::github(&host, account_id) {
                    Ok(identity) => Some(identity),
                    Err(error) => {
                        let _ = self.purge_private();
                        return Err(error);
                    }
                }
            }
            None => None,
            _ => {
                let _ = self.purge_private();
                return Err("Remote navigation identity must be a numeric GitHub scope".into());
            }
        };
        if self.identity == identity {
            return Ok(());
        }
        let advanced = self.advance();
        self.identity = identity;
        self.pending = None;
        self.history
            .retain(|entry| entry.bookmark.scope.allowed(self.identity.as_ref()));
        if self
            .current
            .as_ref()
            .is_some_and(|entry| !entry.bookmark.scope.allowed(self.identity.as_ref()))
        {
            self.current = None;
        }
        if advanced.is_err() {
            // Privacy loss must still purge data after counter exhaustion.
            // Keep the exhausted counter (no wrap/reuse); retire old links.
            self.owner = Arc::new(());
        }
        advanced
    }
    /// Host-global pages have unknown repository membership and are purged
    /// conservatively. Exact unrelated repo/local bookmarks survive unchanged.
    pub fn revoke_repository(
        &mut self,
        scope: &NavigationScope,
        repository: &str,
    ) -> Result<(), String> {
        let scope = match scope {
            NavigationScope::GitHub { host, account_id } => {
                match NavigationScope::github(host, *account_id) {
                    Ok(scope) => scope,
                    Err(error) => {
                        let _ = self.purge_private();
                        return Err(error);
                    }
                }
            }
            _ => {
                let _ = self.purge_private();
                return Err("Repository revocation needs its original numeric GitHub scope".into());
            }
        };
        let repository = match canonical_repository(repository) {
            Ok(repository) => repository,
            Err(error) => {
                let _ = self.purge_private();
                return Err(error);
            }
        };
        let affected = self
            .history
            .iter()
            .any(|entry| entry.bookmark.affected_repository(&scope, &repository))
            || self
                .current
                .as_ref()
                .is_some_and(|entry| entry.bookmark.affected_repository(&scope, &repository))
            || self.pending.as_ref().is_some_and(|pending| {
                pending.destination.affected_repository(&scope, &repository)
            });
        if !affected {
            return Ok(());
        }
        let advanced = self.advance();
        self.history
            .retain(|entry| !entry.bookmark.affected_repository(&scope, &repository));
        if self
            .current
            .as_ref()
            .is_some_and(|entry| entry.bookmark.affected_repository(&scope, &repository))
        {
            self.current = None;
        }
        self.pending = None;
        if advanced.is_err() {
            self.owner = Arc::new(());
        }
        advanced
    }
    fn purge_private(&mut self) -> Result<(), String> {
        let advanced = self.advance();
        self.identity = None;
        self.pending = None;
        self.history
            .retain(|entry| !matches!(entry.bookmark.scope, NavigationScope::GitHub { .. }));
        if self
            .current
            .as_ref()
            .is_some_and(|entry| matches!(entry.bookmark.scope, NavigationScope::GitHub { .. }))
        {
            self.current = None;
        }
        if advanced.is_err() {
            self.owner = Arc::new(());
        }
        advanced
    }
    fn stage(
        &mut self,
        destination: NavigationBookmark,
        direction: Direction,
    ) -> Result<NavigationToken, String> {
        self.ensure_allowed(&destination)?;
        self.advance()?;
        self.pending = Some(Pending {
            epoch: self.epoch,
            destination,
            direction,
        });
        Ok(NavigationToken {
            owner: Arc::clone(&self.owner),
            epoch: self.epoch,
        })
    }
    fn ensure_allowed(&self, bookmark: &NavigationBookmark) -> Result<(), String> {
        if !bookmark.scope.allowed(self.identity.as_ref()) {
            return Err("Private navigation account changed or is unavailable; original page remains hidden".into());
        }
        Ok(())
    }
    fn check(&self, token: &NavigationToken) -> Result<&Pending, String> {
        if !Arc::ptr_eq(&self.owner, &token.owner) || token.epoch != self.epoch {
            return Err("Navigation request was superseded or its scope changed".into());
        }
        self.pending
            .as_ref()
            .filter(|pending| pending.epoch == token.epoch)
            .ok_or_else(|| "Navigation request is no longer pending".into())
    }
    fn advance(&mut self) -> Result<(), String> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("Navigation epoch exhausted; start a new session")?;
        Ok(())
    }
}
fn canonical_host(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 253
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
    {
        return Err(
            "Navigation host must be the bounded original host, not a URL or credential".into(),
        );
    }
    Ok(value.to_ascii_lowercase())
}
fn canonical_repository(value: &str) -> Result<String, String> {
    let mut parts = value.split('/');
    if value.len() > 512
        || !safe_line(value)
        || value.chars().any(char::is_whitespace)
        || parts.next().is_none_or(str::is_empty)
        || parts.next().is_none_or(str::is_empty)
        || parts.next().is_some()
    {
        return Err("Navigation repository needs exact bounded owner/name identity".into());
    }
    Ok(value.to_ascii_lowercase())
}
fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}
fn safe_line(value: &str) -> bool {
    !value.chars().any(|character| character.is_control()
        || matches!(character, '\u{2028}' | '\u{2029}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
}

#[cfg(test)]
#[path = "navigation_trail_tests.rs"]
mod tests;
