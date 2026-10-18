//! Honest Git/GitHub identity and bounded public avatar assets, separate from GitHub auth/API.
use crate::{
    git::{display_bytes, GitActor},
    providers::{
        gh::Actor,
        runner::{CancellationToken, CommandRunner, CommandSpec},
    },
    theme::{ColorMode, SemanticToken, Theme},
};
use image::{ImageFormat, ImageReader, Limits};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    text::Line,
    widgets::{Paragraph, Widget},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashMap},
    fmt, fs,
    hash::{Hash, Hasher},
    io::{Cursor, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

pub const MAX_AVATAR_BYTES: usize = 1024 * 1024;
pub const MAX_AVATAR_DIMENSION: u32 = 512;
const CACHE_DIMENSION: u32 = 64;
const MAX_CACHE_RECORD_BYTES: u64 = 128 * 1024;
const MAX_CACHE_RECORDS: usize = 128;
const MAX_WORKERS: usize = 4;
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarError(pub String);
impl fmt::Display for AvatarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AvatarError {}
impl From<std::io::Error> for AvatarError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitRole {
    Author,
    Committer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorKind {
    User,
    Organization,
    Bot,
    Deleted,
    Unknown,
    Local,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActorIdentity {
    pub host: Option<String>,
    pub login: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub kind: ActorKind,
    pub git_role: Option<GitRole>,
    pub host_associated: bool,
}
impl ActorIdentity {
    pub fn github(host: &str, actor: &Actor) -> Self {
        if let Some(unknown) = crate::actor_avatar_projection_v1::display_fallback(host, actor) {
            return unknown;
        }
        let login = display_bytes(actor.login.as_bytes());
        let kind = if actor.actor_type.eq_ignore_ascii_case("deleted") || login == "ghost" {
            ActorKind::Deleted
        } else if login.is_empty() {
            ActorKind::Unknown
        } else if actor.actor_type.eq_ignore_ascii_case("bot") {
            ActorKind::Bot
        } else if actor.actor_type.eq_ignore_ascii_case("user") {
            ActorKind::User
        } else if actor.actor_type.eq_ignore_ascii_case("organization") {
            ActorKind::Organization
        } else {
            ActorKind::Unknown
        };
        Self {
            host: Some(host.to_owned()),
            login: (!login.is_empty()).then_some(login),
            name: actor
                .display_name
                .as_deref()
                .map(|name| display_bytes(name.as_bytes()))
                .unwrap_or_default(),
            email: None,
            avatar_url: actor.avatar_url.clone(),
            kind,
            git_role: None,
            host_associated: true,
        }
    }
    pub fn local(actor: &GitActor, role: GitRole) -> Self {
        if let Some(unavailable) = crate::git_actor_labels_v1::local_fallback(actor, role) {
            return unavailable;
        }
        Self {
            host: None,
            login: None,
            name: actor.name.clone(),
            email: Some(actor.email.clone()),
            avatar_url: None,
            kind: ActorKind::Local,
            git_role: Some(role),
            host_associated: false,
        }
    }
    pub fn associate(
        mut self,
        commit_id: &str,
        association: &CommitAssociation,
    ) -> Result<Self, AvatarError> {
        if self.git_role != Some(association.role)
            || commit_id != association.commit_id
            || self.name != association.local_name
            || self.email.as_deref() != Some(association.local_email.as_str())
        {
            return Err(AvatarError(
                "GitHub association does not match this exact commit, Git role and local identity"
                    .into(),
            ));
        }
        self.host = Some(association.host.clone());
        self.login = Some(association.actor.login.clone());
        self.avatar_url = association.actor.avatar_url.clone();
        self.host_associated = true;
        self.kind = Self::github(&association.host, &association.actor).kind;
        Ok(self)
    }
    pub fn label(&self) -> String {
        if self.git_role.is_some() && self.login.is_none() && !self.host_associated {
            return crate::git_actor_labels_v1::local_label(
                &self.name,
                self.email.as_deref(),
                self.git_role,
            );
        }
        let status = match self.kind {
            ActorKind::Organization => " [organization]",
            ActorKind::Bot => " [bot]",
            ActorKind::Deleted => " [deleted]",
            ActorKind::Unknown => " [unknown]",
            _ => "",
        };
        let git = match self.git_role {
            Some(GitRole::Author) => "author ",
            Some(GitRole::Committer) => "committer ",
            None => "",
        };
        match (&self.login, &self.email) {
            (Some(login), Some(email)) => {
                format!("{git}{} <{email}> · @{login}{status}", self.name)
            }
            (Some(login), None) => format!("@{login}{status}"),
            (None, Some(email)) => format!(
                "{git}{} <{email}> [local Git]",
                if self.name.is_empty() {
                    "unknown name"
                } else {
                    &self.name
                }
            ),
            (None, None) => format!("unknown actor{status}"),
        }
    }
    pub fn initials(&self) -> String {
        if self.kind == ActorKind::Deleted {
            return "×".into();
        }
        if self.kind == ActorKind::Unknown {
            return "?".into();
        }
        if self.kind == ActorKind::Local {
            return crate::git_actor_labels_v1::local_initials(&self.name);
        }
        let source = if self.name.is_empty() {
            self.login.as_deref().unwrap_or("?")
        } else {
            &self.name
        };
        let parts = source
            .split(|ch: char| ch.is_whitespace() || ch == '-' || ch == '_')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let chars = if parts.len() > 1 {
            [
                parts.first().and_then(|part| part.chars().next()),
                parts.last().and_then(|part| part.chars().next()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
        } else {
            source.chars().take(2).collect()
        };
        chars
            .into_iter()
            .flat_map(char::to_uppercase)
            .take(2)
            .collect()
    }
    fn asset_key(&self, scope: &str) -> Option<String> {
        self.avatar_url.as_ref().map(|url| {
            format!(
                "{scope}\0{:?}\0{:?}\0{}\0{url}",
                self.host, self.login, self.name
            )
        })
    }
}

/// An exact-SHA association returned by gh, distinct from cryptographic signature verification.
#[derive(Debug, Clone)]
pub struct CommitAssociation {
    pub host: String,
    pub repository: String,
    pub commit_id: String,
    pub role: GitRole,
    pub local_name: String,
    pub local_email: String,
    pub actor: Actor,
}
impl CommitAssociation {
    pub fn from_response(
        host: &str,
        repository: &str,
        commit_id: &str,
        role: GitRole,
        response: &Value,
    ) -> Result<Self, AvatarError> {
        if !matches!(commit_id.len(), 40 | 64)
            || !commit_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || response.get("sha").and_then(Value::as_str) != Some(commit_id)
        {
            return Err(AvatarError(
                "Commit association requires the exact inspected SHA".into(),
            ));
        }
        let field = match role {
            GitRole::Author => "author",
            GitRole::Committer => "committer",
        };
        let remote = response
            .get(field)
            .ok_or_else(|| AvatarError("No GitHub account association".into()))?;
        let login = remote
            .get("login")
            .and_then(Value::as_str)
            .filter(|login| !login.is_empty())
            .ok_or_else(|| AvatarError("No GitHub account association".into()))?;
        let local = response
            .get("commit")
            .and_then(|commit| commit.get(field))
            .ok_or_else(|| AvatarError("Missing local Git provenance".into()))?;
        Ok(Self {
            host: host.into(),
            repository: repository.into(),
            commit_id: commit_id.into(),
            role,
            local_name: display_bytes(
                local
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            local_email: display_bytes(
                local
                    .get("email")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            actor: Actor {
                login: display_bytes(login.as_bytes()),
                display_name: remote
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                avatar_url: remote
                    .get("avatar_url")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                actor_type: remote
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("User")
                    .to_owned(),
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct AssetPolicy {
    pub trusted_hosts: BTreeSet<String>,
}
impl Default for AssetPolicy {
    fn default() -> Self {
        Self {
            trusted_hosts: BTreeSet::from([
                "avatars.githubusercontent.com".into(),
                "github.com".into(),
            ]),
        }
    }
}
impl AssetPolicy {
    pub fn validate(&self, url: &str) -> Result<(), AvatarError> {
        if url.len() > 2048
            || url
                .bytes()
                .any(|byte| !byte.is_ascii_graphic() || byte == b'\\' || byte == b'#')
        {
            return Err(AvatarError(
                "Avatar URL contains unsafe or oversized text".into(),
            ));
        }
        let rest = url
            .strip_prefix("https://")
            .ok_or_else(|| AvatarError("Public avatars require HTTPS".into()))?;
        let host = rest.split(['/', '?']).next().unwrap_or_default();
        if host.is_empty()
            || host
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && !b".-".contains(&byte))
            || !self.trusted_hosts.contains(&host.to_ascii_lowercase())
        {
            return Err(AvatarError(
                "Avatar host is not trusted; using initials".into(),
            ));
        }
        if let Some(query) = url.split_once('?').map(|(_, query)| query) {
            for part in query.split('&') {
                let name = part
                    .split('=')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if [
                    "token",
                    "access_token",
                    "auth",
                    "authorization",
                    "key",
                    "password",
                ]
                .contains(&name.as_str())
                {
                    return Err(AvatarError(
                        "Authenticated avatar URLs are not accepted".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AvatarImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}
impl AvatarImage {
    pub fn decode(bytes: &[u8]) -> Result<Self, AvatarError> {
        if bytes.is_empty() || bytes.len() > MAX_AVATAR_BYTES {
            return Err(AvatarError(
                "Avatar exceeds its 1 MiB byte limit or is empty".into(),
            ));
        }
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| AvatarError(error.to_string()))?;
        if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
            return Err(AvatarError("Avatar format must be PNG or JPEG".into()));
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_AVATAR_DIMENSION);
        limits.max_image_height = Some(MAX_AVATAR_DIMENSION);
        limits.max_alloc = Some(4 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|error| AvatarError(format!("Invalid or oversized avatar: {error}")))?;
        let image = if decoded.width() > CACHE_DIMENSION || decoded.height() > CACHE_DIMENSION {
            decoded
                .thumbnail(CACHE_DIMENSION, CACHE_DIMENSION)
                .to_rgba8()
        } else {
            decoded.to_rgba8()
        };
        let result = Self {
            width: image.width(),
            height: image.height(),
            pixels: image.pixels().map(|pixel| pixel.0).collect(),
        };
        result.validate()?;
        Ok(result)
    }
    fn validate(&self) -> Result<(), AvatarError> {
        if self.width == 0
            || self.height == 0
            || self.width > CACHE_DIMENSION
            || self.height > CACHE_DIMENSION
            || self.pixels.len() != self.width as usize * self.height as usize
        {
            return Err(AvatarError("Invalid cached avatar pixel bounds".into()));
        }
        Ok(())
    }
    fn sample(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y.min(self.height - 1) * self.width + x.min(self.width - 1)) as usize]
    }
}

/// A bounded, worker-decoded artifact raster. This is not an actor avatar and does not relax
/// AvatarImage::decode's smaller public-asset limits or perform any image/network I/O in draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDisplay {
    pub original_width: u32,
    pub original_height: u32,
    pub format: String,
    pub raster: AvatarImage,
}
impl ImageDisplay {
    pub fn decode(bytes: &[u8]) -> Result<Self, AvatarError> {
        Self::decode_limited_v2(bytes, 4096, 64 * 1024 * 1024)
    }
    /// SAME decoder, narrower Actions-only phase bound. Legacy artifact and
    /// avatar defaults remain unchanged; no network/source authority here.
    pub(crate) fn decode_actions_preview_v2(bytes: &[u8]) -> Result<Self, AvatarError> {
        Self::decode_limited_v2(bytes, 1024, 8 * 1024 * 1024)
    }
    fn decode_limited_v2(
        bytes: &[u8],
        dimension: u32,
        allocation: u64,
    ) -> Result<Self, AvatarError> {
        if bytes.is_empty() || bytes.len() > 256 * 1024 {
            return Err(AvatarError(
                "Image preview must be complete and no larger than256KiB".into(),
            ));
        }
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| AvatarError(error.to_string()))?;
        let format = match reader.format() {
            Some(ImageFormat::Png) => "PNG",
            Some(ImageFormat::Jpeg) => "JPEG",
            _ => return Err(AvatarError("Image preview supports PNG/JPEG only".into())),
        };
        let mut limits = Limits::default();
        limits.max_image_width = Some(dimension);
        limits.max_image_height = Some(dimension);
        limits.max_alloc = Some(allocation);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|error| AvatarError(format!("Invalid or oversized image preview: {error}")))?;
        let source_width = decoded.width();
        let source_height = decoded.height();
        let small = if source_width > CACHE_DIMENSION || source_height > CACHE_DIMENSION {
            decoded
                .thumbnail(CACHE_DIMENSION, CACHE_DIMENSION)
                .to_rgba8()
        } else {
            decoded.to_rgba8()
        };
        let raster = AvatarImage {
            width: small.width(),
            height: small.height(),
            pixels: small.pixels().map(|pixel| pixel.0).collect(),
        };
        raster.validate()?;
        Ok(Self {
            original_width: source_width,
            original_height: source_height,
            format: format.into(),
            raster,
        })
    }
}

/// Native half-block image canvas with aspect-aware bounded sampling and explicit terminal fallback.
pub struct ImageWidget<'a> {
    pub image: &'a ImageDisplay,
    pub theme: Theme,
}
impl Widget for ImageWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        if self.theme.mode != ColorMode::TrueColor || self.image.raster.validate().is_err() {
            Paragraph::new(format!(
                "{} image {}x{} • color preview needs a truecolor terminal",
                self.image.format, self.image.original_width, self.image.original_height
            ))
            .style(self.theme.style(SemanticToken::Muted))
            .render(area, buffer);
            return;
        }
        let image = &self.image.raster;
        let mut width = u32::from(area.width).min(image.width);
        let mut height = (width * image.height).div_ceil(image.width * 2).max(1);
        if height > u32::from(area.height) {
            height = u32::from(area.height);
            width = (height * 2 * image.width / image.height).max(1).min(width);
        }
        for y in 0..height {
            for x in 0..width {
                let px = x * image.width / width;
                let top = y * 2 * image.height / (height * 2);
                let bottom = (y * 2 + 1) * image.height / (height * 2);
                let backdrop = self.theme.backdrop();
                let color = |pixel: [u8; 4]| {
                    let channel = |value: u8, under: u8| {
                        ((u16::from(value) * u16::from(pixel[3])
                            + u16::from(under) * (255 - u16::from(pixel[3])))
                            / 255) as u8
                    };
                    Color::Rgb(
                        channel(pixel[0], backdrop.0),
                        channel(pixel[1], backdrop.1),
                        channel(pixel[2], backdrop.2),
                    )
                };
                buffer[(area.x + x as u16, area.y + y as u16)]
                    .set_symbol("▀")
                    .set_fg(color(image.sample(px, top)))
                    .set_bg(color(image.sample(px, bottom)));
            }
        }
    }
}

pub trait AvatarFetcher: Send + Sync {
    fn fetch(&self, url: &str, cancellation: &CancellationToken) -> Result<Vec<u8>, AvatarError>;
}

#[derive(Clone)]
pub struct CurlAvatarFetcher {
    program: PathBuf,
    policy: AssetPolicy,
    runner: CommandRunner,
}
impl CurlAvatarFetcher {
    /// Curl is an optional image transport. Its absence leaves avatars as initials.
    pub fn detect(policy: AssetPolicy) -> Option<Self> {
        std::env::var_os("PATH")
            .and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|directory| directory.join("curl"))
                    .find(|path| path.is_file())
            })
            .map(|program| Self {
                program,
                policy,
                runner: CommandRunner::default(),
            })
    }
    pub fn with_program(program: PathBuf, policy: AssetPolicy) -> Self {
        Self {
            program,
            policy,
            runner: CommandRunner::default(),
        }
    }
}
impl AvatarFetcher for CurlAvatarFetcher {
    fn fetch(&self, url: &str, cancellation: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        self.policy.validate(url)?;
        let mut spec = CommandSpec::new(self.program.as_os_str())
            .args([
                "-q",
                "--fail",
                "--silent",
                "--show-error",
                "--proto",
                "=https",
                "--max-redirs",
                "0",
                "--connect-timeout",
                "3",
                "--max-time",
                "6",
                "--max-filesize",
                "1048576",
                "--header",
                "Accept: image/png,image/jpeg",
                "--url",
                url,
            ])
            .timeout(Duration::from_secs(7));
        spec.clear_env = true;
        spec.output_limit = MAX_AVATAR_BYTES;
        self.runner
            .run(spec, cancellation)
            .map(|output| output.stdout)
            .map_err(|error| {
                AvatarError(format!(
                    "Avatar unavailable; initials remain visible: {error}"
                ))
            })
    }
}

#[derive(Clone, Debug)]
pub enum AvatarState {
    Absent,
    Loading,
    Busy,
    Ready(Arc<AvatarImage>),
    Failed(String),
}
struct CacheEntry {
    state: AvatarState,
    cancellation: Option<CancellationToken>,
    touched: SystemTime,
}
#[derive(Serialize, Deserialize)]
struct DiskAvatar {
    key: String,
    image: AvatarImage,
}
struct CacheInner {
    entries: HashMap<String, CacheEntry>,
    workers: usize,
    handles: Vec<std::thread::JoinHandle<()>>,
    shutting_down: bool,
}
#[derive(Clone)]
pub struct AvatarCache {
    directory: PathBuf,
    scope: String,
    fetcher: Arc<dyn AvatarFetcher>,
    inner: Arc<Mutex<CacheInner>>,
}
impl AvatarCache {
    /// Allocation identity only. Never proves source, lease or account access;
    /// a newer surface can legitimately share this very same allocation.
    pub(crate) fn same_instance_v7(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
    /// Metadata partition only, never current authentication/access authority.
    pub(crate) fn scope_key(&self) -> &str {
        &self.scope
    }
    pub fn new(directory: PathBuf, scope: String, fetcher: Arc<dyn AvatarFetcher>) -> Self {
        Self {
            directory,
            scope,
            fetcher,
            inner: Arc::new(Mutex::new(CacheInner {
                entries: HashMap::new(),
                workers: 0,
                handles: Vec::new(),
                shutting_down: false,
            })),
        }
    }
    /// Fast, nonblocking deduplicated request. Disk reads, fetch and decoding occur in workers.
    pub fn request(&self, identity: &ActorIdentity) -> AvatarState {
        let Some(key) = identity.asset_key(&self.scope) else {
            return AvatarState::Absent;
        };
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.shutting_down {
            return AvatarState::Failed(
                "Avatar cache has shut down; initials remain visible".into(),
            );
        }
        if let Some(entry) = inner.entries.get_mut(&key) {
            entry.touched = SystemTime::now();
            return entry.state.clone();
        }
        if inner.workers >= MAX_WORKERS {
            return AvatarState::Busy;
        }
        if inner.entries.len() >= MAX_CACHE_RECORDS {
            let oldest = inner
                .entries
                .iter()
                .filter(|(_, entry)| entry.cancellation.is_none())
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                inner.entries.remove(&oldest);
            } else {
                return AvatarState::Busy;
            }
        }
        let cancellation = CancellationToken::default();
        inner.entries.insert(
            key.clone(),
            CacheEntry {
                state: AvatarState::Loading,
                cancellation: Some(cancellation.clone()),
                touched: SystemTime::now(),
            },
        );
        inner.workers += 1;
        inner.handles.retain(|handle| !handle.is_finished());
        let cache = self.clone();
        let url = identity.avatar_url.clone().unwrap_or_default();
        let handle = std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cache.load_or_fetch(&key, &url, &cancellation)
            }))
            .unwrap_or_else(|_| {
                Err(AvatarError(
                    "Avatar worker failed; initials remain visible".into(),
                ))
            });
            let mut inner = cache
                .inner
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            inner.workers = inner.workers.saturating_sub(1);
            if let Some(entry) = inner.entries.get_mut(&key) {
                entry.cancellation = None;
                entry.state = match result {
                    Ok(image) => AvatarState::Ready(Arc::new(image)),
                    Err(error) => AvatarState::Failed(error.to_string()),
                };
            }
        });
        // Register under the same lock as workers/tokens: shutdown cannot miss a spawned child.
        inner.handles.push(handle);
        drop(inner);
        AvatarState::Loading
    }
    pub fn cancel_pending(&self) {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        for entry in inner.entries.values() {
            if let Some(token) = &entry.cancellation {
                token.cancel();
            }
        }
    }
    /// Cancel and drain all workers before process exit or identity-scope replacement.
    /// Production fetchers use the bounded cancellable command runner; custom fetchers must
    /// likewise honor cancellation. Never call this from drawing or while holding application locks.
    pub fn shutdown(&self) {
        let handles = {
            let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
            inner.shutting_down = true;
            for entry in inner.entries.values() {
                if let Some(token) = &entry.cancellation {
                    token.cancel();
                }
            }
            std::mem::take(&mut inner.handles)
        };
        for handle in handles {
            let _ = handle.join();
        }
    }
    pub fn retry(&self, identity: &ActorIdentity) {
        if let Some(key) = identity.asset_key(&self.scope) {
            let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
            if inner
                .entries
                .get(&key)
                .is_some_and(|entry| matches!(entry.state, AvatarState::Failed(_)))
            {
                inner.entries.remove(&key);
            }
        }
    }
    fn filename(&self, key: &str) -> PathBuf {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hash);
        self.directory
            .join(format!("avatar-{:016x}.json", hash.finish()))
    }
    fn load_or_fetch(
        &self,
        key: &str,
        url: &str,
        cancellation: &CancellationToken,
    ) -> Result<AvatarImage, AvatarError> {
        if cancellation.is_cancelled() {
            return Err(AvatarError("Avatar cancelled".into()));
        }
        let path = self.filename(key);
        if let Ok(metadata) = fs::metadata(&path) {
            if metadata.len() <= MAX_CACHE_RECORD_BYTES
                && metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.elapsed().ok())
                    .is_some_and(|age| age < CACHE_TTL)
            {
                let mut bytes = Vec::new();
                if fs::File::open(&path)?
                    .take(MAX_CACHE_RECORD_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .is_ok()
                {
                    if let Ok(record) = serde_json::from_slice::<DiskAvatar>(&bytes) {
                        if record.key == key && record.image.validate().is_ok() {
                            return Ok(record.image);
                        }
                    }
                }
            }
        }
        let bytes = self.fetcher.fetch(url, cancellation)?;
        if cancellation.is_cancelled() {
            return Err(AvatarError("Avatar cancelled".into()));
        }
        let image = AvatarImage::decode(&bytes)?;
        // Cache persistence is best effort; valid fetched identity still renders if storage is read-only.
        let persist = || -> Result<(), AvatarError> {
            fs::create_dir_all(&self.directory)?;
            let bytes = serde_json::to_vec(&DiskAvatar {
                key: key.into(),
                image: image.clone(),
            })
            .map_err(|error| AvatarError(error.to_string()))?;
            if bytes.len() > MAX_CACHE_RECORD_BYTES as usize {
                return Err(AvatarError("Avatar cache record exceeds bound".into()));
            }
            let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
            temporary.write_all(&bytes)?;
            temporary
                .persist(&path)
                .map_err(|error| AvatarError(error.to_string()))?;
            self.prune_disk();
            Ok(())
        };
        let _ = persist();
        Ok(image)
    }

    /// Remove only validated files produced by this avatar cache, bounded across account scopes.
    fn prune_disk(&self) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        let mut owned = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.len() != 28
                || !name.starts_with("avatar-")
                || !name.ends_with(".json")
                || !name[7..23].bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_file() || metadata.len() > MAX_CACHE_RECORD_BYTES {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<DiskAvatar>(&bytes) else {
                continue;
            };
            if record.image.validate().is_err() || self.filename(&record.key) != path {
                continue;
            }
            owned.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), path));
        }
        owned.sort_by_key(|(time, _)| *time);
        let remove_count = owned.len().saturating_sub(MAX_CACHE_RECORDS);
        for (_, path) in owned.into_iter().take(remove_count) {
            let _ = fs::remove_file(path);
        }
    }
}

pub struct AvatarWidget<'a> {
    pub identity: &'a ActorIdentity,
    pub image: Option<&'a AvatarImage>,
    pub theme: Theme,
}
impl Widget for AvatarWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        if self.theme.mode != ColorMode::TrueColor
            || self.image.is_none_or(|image| image.validate().is_err())
        {
            Paragraph::new(format!("[{}]", self.identity.initials()))
                .style(self.theme.style(SemanticToken::Actor))
                .render(area, buffer);
            return;
        }
        let image = self.image.unwrap();
        for y in 0..area.height {
            for x in 0..area.width {
                let px = u32::from(x) * image.width / u32::from(area.width);
                let top_y = u32::from(y) * 2 * image.height / (u32::from(area.height) * 2);
                let bottom_y = (u32::from(y) * 2 + 1) * image.height / (u32::from(area.height) * 2);
                let backdrop = self.theme.backdrop();
                let blend = |pixel: [u8; 4]| -> Color {
                    let channel = |value: u8, under: u8| {
                        ((u16::from(value) * u16::from(pixel[3])
                            + u16::from(under) * (255 - u16::from(pixel[3])))
                            / 255) as u8
                    };
                    Color::Rgb(
                        channel(pixel[0], backdrop.0),
                        channel(pixel[1], backdrop.1),
                        channel(pixel[2], backdrop.2),
                    )
                };
                buffer[(area.x + x, area.y + y)]
                    .set_symbol("▀")
                    .set_fg(blend(image.sample(px, top_y)))
                    .set_bg(blend(image.sample(px, bottom_y)));
            }
        }
    }
}

/// Username text is always present. Images occupy only their layout-assigned rectangle.
pub struct ActorWidget<'a> {
    pub identity: &'a ActorIdentity,
    pub image: Option<&'a AvatarImage>,
    pub theme: Theme,
}
impl Widget for ActorWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let label = self.identity.label();
        if area.width < 10 {
            Paragraph::new(label)
                .style(self.theme.style(SemanticToken::Actor))
                .render(area, buffer);
            return;
        }
        let avatar_area = Rect::new(area.x, area.y, 4, area.height.min(2));
        AvatarWidget {
            identity: self.identity,
            image: self.image,
            theme: self.theme,
        }
        .render(avatar_area, buffer);
        let text_area = Rect::new(area.x + 5, area.y, area.width - 5, area.height);
        let mut lines = vec![Line::from(label)];
        if area.height > 1 && !self.identity.name.is_empty() && self.identity.git_role.is_none() {
            lines.push(Line::from(self.identity.name.clone()));
        }
        Paragraph::new(lines)
            .style(self.theme.style(SemanticToken::Actor))
            .render(text_area, buffer);
    }
}
