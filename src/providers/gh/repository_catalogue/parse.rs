use super::*;
use std::io::{self, Write};

/// Count serialized raw object bytes with no allocation/cloning of a large row.
struct ByteCount {
    bytes: usize,
    cap: usize,
}
impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.cap.saturating_sub(self.bytes) {
            return Err(io::Error::other("catalogue raw row byte cap"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn text<'a>(value: &'a Value, field: &str, cap: usize) -> Result<Option<&'a str>, &'static str> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.len() <= cap => Ok(Some(text)),
        _ => Err("Observed repository scalar has an invalid type or raw byte cap; row excluded"),
    }
}
fn node(value: &Value) -> Result<Option<&str>, &'static str> {
    let node = text(value, "node_id", 1024)?;
    if node.is_some_and(|node| node.is_empty() || node.chars().any(char::is_control)) {
        return Err("Observed opaque node identity is invalid; no ID prefix/string inference");
    }
    Ok(node)
}
fn optional_id(value: &Value) -> Result<Option<u64>, &'static str> {
    match value.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(id) => id
            .as_u64()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or("Observed optional actor ID is not a positive REST numeric value"),
    }
}
pub(super) fn owner(
    value: &Value,
    source: &RepositoryCatalogueSource,
    requested: &str,
) -> GithubResult<RepositoryCatalogueOwner> {
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Catalogue owner endpoint omitted genuine positive database identity",
            )
        })?;
    let login = text(value, "login", 100)
        .map_err(|reason| error(GithubErrorKind::Json, reason))?
        .filter(|login| !login.is_empty())
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Catalogue owner lacks bounded canonical login",
            )
        })?;
    validate_name(login)?;
    if !login.eq_ignore_ascii_case(requested) {
        return Err(error(GithubErrorKind::IdentityChanged,"Catalogue owner changed its canonical name; select the actual source explicitly without guessing redirects"));
    }
    let kind = match source {
        RepositoryCatalogueSource::Organization(_) => RepositoryCatalogueOwnerKind::Organization,
        RepositoryCatalogueSource::PublicUser(_) => RepositoryCatalogueOwnerKind::User,
        RepositoryCatalogueSource::AuthenticatedAccessible => {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Authenticated-accessible inventory has no invented single owner",
            ))
        }
    };
    if let Some(actual) =
        text(value, "type", 100).map_err(|reason| error(GithubErrorKind::Json, reason))?
    {
        let expected = match kind {
            RepositoryCatalogueOwnerKind::Organization => "Organization",
            RepositoryCatalogueOwnerKind::User => "User",
        };
        if actual != expected {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Owner endpoint namespace contradicts selected source kind",
            ));
        }
    }
    Ok(RepositoryCatalogueOwner {
        id,
        node_id: node(value)
            .map_err(|reason| error(GithubErrorKind::Json, reason))?
            .map(str::to_owned),
        login: login.into(),
        kind,
    })
}
fn flag(value: &Value, field: &str, limitations: &mut Vec<String>) -> Option<bool> {
    match value.get(field) {
        Some(Value::Bool(flag)) => Some(*flag),
        None | Some(Value::Null) => None,
        _ => {
            limitations.push(format!("{field} malformed: unknown, not false"));
            None
        }
    }
}

/// Closed parser-only context. It never constructs a source/read receipt or
/// grants access; catalogue provenance and PublicUser privacy stay explicit.
pub(super) struct RepositoryParseContextV13<'a> {
    expected_owner: Option<&'a RepositoryCatalogueOwner>,
    public_user: bool,
}
impl<'a> RepositoryParseContextV13<'a> {
    fn catalogue(context: &'a RepositoryCatalogueContext) -> Self {
        Self {
            expected_owner: context.owner.as_ref(),
            public_user: matches!(context.source, RepositoryCatalogueSource::PublicUser(_)),
        }
    }
    pub(super) fn current_catalogue(
        original: &RepositoryCatalogueContext,
        current_owner: &'a RepositoryCatalogueOwner,
    ) -> Self {
        Self {
            expected_owner: original.owner.as_ref().map(|_| current_owner),
            public_user: matches!(original.source, RepositoryCatalogueSource::PublicUser(_)),
        }
    }
}
pub(super) fn repository(
    value: &Value,
    context: &RepositoryCatalogueContext,
    max_bytes: usize,
    metadata_left: usize,
) -> Result<(RepositoryCatalogueRepository, usize), &'static str> {
    repository_with_context_v13(
        value,
        &RepositoryParseContextV13::catalogue(context),
        max_bytes,
        metadata_left,
    )
}
pub(super) fn repository_with_context_v13(
    value: &Value,
    context: &RepositoryParseContextV13<'_>,
    max_bytes: usize,
    metadata_left: usize,
) -> Result<(RepositoryCatalogueRepository, usize), &'static str> {
    if !value.is_object() {
        return Err("Observed repository row is not an object; membership partial");
    }
    let mut counter = ByteCount {
        bytes: 0,
        cap: max_bytes,
    };
    serde_json::to_writer(&mut counter, value).map_err(|_| {
        "Observed raw repository row exceeds selected document cap before display/metadata clones"
    })?;
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or("Repository requires an actual positive REST numeric ID; no name/node fallback")?;
    let full_name = text(value, "full_name", 256)?
        .filter(|name| !name.is_empty())
        .ok_or("Repository omitted canonical full_name")?;
    validate_repo(full_name)
        .map_err(|_| "Repository canonical name is not a bounded owner/name")?;
    let repository_node = node(value)?;
    let owner_name = full_name
        .split('/')
        .next()
        .ok_or("Repository owner missing")?;
    if context
        .expected_owner
        .as_ref()
        .is_some_and(|owner| !owner.login.eq_ignore_ascii_case(owner_name))
    {
        return Err(
            "Repository belongs to a different actual canonical owner than the requested source",
        );
    }
    let description = text(value, "description", 16 * 1024)?;
    let branch = text(value, "default_branch", 4096)?;
    if branch.is_some_and(|branch| branch.is_empty() || branch.chars().any(char::is_control)) {
        return Err("Default branch is malformed; no guessed main or sanitized ref substitution");
    }
    let language = text(value, "language", 1024)?;
    let updated = text(value, "updated_at", 1024)?;
    let raw_owner = match value.get("owner") {
        None | Some(Value::Null) => None,
        Some(owner) if owner.is_object() => Some(owner),
        _ => return Err("Repository owner metadata is malformed"),
    };
    // Include conservative row/map overhead and separate canonical name/node
    // dictionary copies in the admission ledger, not just display string bytes.
    let mut metadata_bytes = 1024
        + full_name.len() * 2
        + repository_node.map_or(0, str::len) * 2
        + description.map_or(0, str::len)
        + branch.map_or(0, str::len)
        + language.map_or(0, str::len)
        + updated.map_or(0, str::len);
    let owner_fields = if let Some(owner) = raw_owner {
        let login = text(owner, "login", 100)?;
        if login.is_some_and(|login| !login.eq_ignore_ascii_case(owner_name)) {
            return Err("Repository owner actor contradicts canonical owner name");
        }
        let actor_id = optional_id(owner)?;
        let actor_node = node(owner)?;
        if let Some(expected) = &context.expected_owner {
            if actor_id.is_some_and(|id| id != expected.id)
                || actor_node
                    .zip(expected.node_id.as_deref())
                    .is_some_and(|(actual, expected)| actual != expected)
            {
                return Err("Repository owner IDs contradict actual selected owner proof");
            }
        }
        let actor_type = text(owner, "type", 128)?;
        if let Some(expected) = &context.expected_owner {
            let expected = match expected.kind {
                RepositoryCatalogueOwnerKind::Organization => "Organization",
                RepositoryCatalogueOwnerKind::User => "User",
            };
            if actor_type.is_some_and(|kind| kind != expected) {
                return Err("Repository owner actor type contradicts original source namespace");
            }
        }
        let name = text(owner, "name", 1024)?;
        let avatar = text(owner, "avatar_url", 4096)?;
        metadata_bytes += login.map_or(0, str::len)
            + actor_node.map_or(0, str::len)
            + actor_type.map_or(0, str::len)
            + name.map_or(0, str::len)
            + avatar.map_or(0, str::len);
        Some((actor_id, actor_node, login, name, avatar, actor_type))
    } else {
        None
    };
    let mut limitations = Vec::new();
    let private = flag(value, "private", &mut limitations);
    if context.public_user && private == Some(true) {
        return Err(
            "Public-user inventory cannot admit a private repository or claim private coverage",
        );
    }
    let fork = flag(value, "fork", &mut limitations);
    let archived = flag(value, "archived", &mut limitations);
    let mut permissions = RepositoryCataloguePermissions::default();
    if let Some(raw) = value.get("permissions").filter(|raw| !raw.is_null()) {
        if raw.is_object() {
            permissions = RepositoryCataloguePermissions {
                pull: flag(raw, "pull", &mut limitations),
                push: flag(raw, "push", &mut limitations),
                maintain: flag(raw, "maintain", &mut limitations),
                admin: flag(raw, "admin", &mut limitations),
                triage: flag(raw, "triage", &mut limitations),
            };
        } else {
            limitations.push("permissions malformed: every flag unknown, not denial".into());
        }
    }
    metadata_bytes += limitations.iter().map(String::len).sum::<usize>();
    if metadata_bytes > metadata_left {
        return Err("Bounded metadata admission cap excluded observed repository before scalar/actor copies; no complete membership claimed");
    }
    let owner = owner_fields.map(|(actor_id, actor_node, login, name, avatar, actor_type)| {
        RepositoryCatalogueActor {
            database_id: actor_id,
            node_id: actor_node.map(str::to_owned),
            actor: Actor {
                login: login.unwrap_or_default().into(),
                display_name: name.map(str::to_owned),
                avatar_url: avatar.map(str::to_owned),
                actor_type: actor_type.unwrap_or("Unknown").into(),
            },
        }
    });
    Ok((
        RepositoryCatalogueRepository {
            id,
            node_id: repository_node.map(str::to_owned),
            full_name: full_name.into(),
            description: description.map(str::to_owned),
            owner,
            default_branch: branch.map(str::to_owned),
            private,
            fork,
            archived,
            language: language.map(str::to_owned),
            permissions,
            updated_at: updated.map(str::to_owned),
            metadata_limitations: limitations,
        },
        metadata_bytes,
    ))
}
pub(super) fn same_observation(
    a: &RepositoryCatalogueRepository,
    b: &RepositoryCatalogueRepository,
) -> bool {
    a == b
}
