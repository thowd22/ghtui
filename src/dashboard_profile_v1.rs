//! Account-scoped dashboard preferences, never cached evidence or access authority.
//!
//! Version 2 keeps several named dashboards per account in one bounded record
//! ([`PROFILES_KEY`]). The original single default profile ([`PROFILE_KEY`], version 1)
//! is still read: while no version-2 record exists it appears as the dashboard named
//! [`DEFAULT_NAME`], and it is never rewritten or deleted.
use crate::{
    metrics::MetricKind,
    storage::{Cache, Scope},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The original single default profile (version 1); read only.
pub const PROFILE_KEY: &str = "ghtui/dashboard/default/v1";
pub const PROFILE_BYTES: usize = 32 * 1024;
/// Named dashboards (version 2).
pub const PROFILES_KEY: &str = "ghtui/dashboard/profiles/v2";
pub const PROFILES_BYTES: usize = 128 * 1024;
pub const MAX_DASHBOARDS: usize = 12;
pub const MAX_NAME_CHARS: usize = 40;
/// The name a converted version-1 profile, and a first save, receive.
pub const DEFAULT_NAME: &str = "Default";
/// The stale-PR thresholds a dashboard may choose, in days.
pub const STALE_THRESHOLDS: [u64; 4] = [3, 7, 14, 30];
pub const DEFAULT_STALE_DAYS: u64 = 7;

/// One Dashboard layout slot: a metric card or an attention widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DashboardWidget {
    OpenPrs,
    NeedsMyReview,
    OpenAge,
    ReviewWait,
    FirstReview,
    MergeLatency,
    Checks,
    Workflows,
    /// Open PRs idle for the dashboard's stale threshold (no gh read).
    StalePrs,
    /// Unresolved review threads, read only on request.
    UnresolvedThreads,
}
impl DashboardWidget {
    pub fn metric(self) -> Option<MetricKind> {
        Some(match self {
            Self::OpenPrs => MetricKind::OpenPrs,
            Self::NeedsMyReview => MetricKind::NeedsMyReview,
            Self::OpenAge => MetricKind::OpenAge,
            Self::ReviewWait => MetricKind::ReviewWait,
            Self::FirstReview => MetricKind::FirstReview,
            Self::MergeLatency => MetricKind::MergeLatency,
            Self::Checks => MetricKind::Checks,
            Self::Workflows => MetricKind::Workflows,
            Self::StalePrs | Self::UnresolvedThreads => return None,
        })
    }
    pub fn from_metric(kind: MetricKind) -> Self {
        match kind {
            MetricKind::OpenPrs => Self::OpenPrs,
            MetricKind::NeedsMyReview => Self::NeedsMyReview,
            MetricKind::OpenAge => Self::OpenAge,
            MetricKind::ReviewWait => Self::ReviewWait,
            MetricKind::FirstReview => Self::FirstReview,
            MetricKind::MergeLatency => Self::MergeLatency,
            MetricKind::Checks => Self::Checks,
            MetricKind::Workflows => Self::Workflows,
        }
    }
    pub fn label(self) -> &'static str {
        WIDGETS
            .iter()
            .find(|(widget, _)| *widget == self)
            .map(|(_, label)| *label)
            .unwrap_or("Dashboard widget")
    }
}
pub const WIDGETS: [(DashboardWidget, &str); 10] = [
    (DashboardWidget::OpenPrs, "Loaded open PRs"),
    (DashboardWidget::NeedsMyReview, "Needs my review"),
    (DashboardWidget::OpenAge, "Mean open PR age"),
    (DashboardWidget::ReviewWait, "Mean requested-review wait"),
    (
        DashboardWidget::FirstReview,
        "Mean first submitted review latency",
    ),
    (DashboardWidget::MergeLatency, "Mean merge latency"),
    (DashboardWidget::Checks, "Observed exact-head check health"),
    (
        DashboardWidget::Workflows,
        "Latest observed workflow-attempt health",
    ),
    (DashboardWidget::StalePrs, "Stale PRs"),
    (DashboardWidget::UnresolvedThreads, "Unresolved threads"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetSlot {
    pub widget: DashboardWidget,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardLayout {
    slots: Vec<WidgetSlot>,
}
/// The name earlier code used for the layout.
pub type DashboardLayoutV1 = DashboardLayout;
impl Default for DashboardLayout {
    fn default() -> Self {
        Self {
            slots: WIDGETS
                .iter()
                .map(|(widget, _)| WidgetSlot {
                    widget: *widget,
                    visible: true,
                })
                .collect(),
        }
    }
}
impl DashboardLayout {
    pub fn slots(&self) -> &[WidgetSlot] {
        &self.slots
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.slots.len() != WIDGETS.len()
            || !self.slots.iter().any(|slot| slot.visible)
            || WIDGETS.iter().any(|(widget, _)| {
                self.slots
                    .iter()
                    .filter(|slot| slot.widget == *widget)
                    .count()
                    != 1
            })
        {
            return Err("Dashboard layout needs each known widget exactly once and at least one shown widget".into());
        }
        Ok(())
    }
    pub fn toggle(&mut self, index: usize) -> Result<(), String> {
        self.validate()?;
        let slot = self
            .slots
            .get(index)
            .ok_or("Dashboard widget is not in this layout")?;
        if slot.visible && self.slots.iter().filter(|slot| slot.visible).count() == 1 {
            return Err("Keep at least one dashboard widget visible".into());
        }
        self.slots[index].visible = !self.slots[index].visible;
        Ok(())
    }
    pub fn move_slot(&mut self, index: usize, down: bool) -> Result<usize, String> {
        self.validate()?;
        if index >= self.slots.len() {
            return Err("Dashboard widget is not in this layout".into());
        }
        let next = if down {
            index.saturating_add(1).min(self.slots.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        self.slots.swap(index, next);
        Ok(next)
    }
    pub fn shown(&self) -> impl Iterator<Item = DashboardWidget> + '_ {
        self.slots
            .iter()
            .filter(|slot| slot.visible)
            .map(|slot| slot.widget)
    }
}

/// The version-1 layout: exactly the eight metric cards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetSlotV1 {
    pub kind: MetricKind,
    pub visible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyLayoutV1 {
    slots: Vec<WidgetSlotV1>,
}
impl LegacyLayoutV1 {
    fn validate(&self) -> Result<(), String> {
        let metrics = WIDGETS
            .iter()
            .filter_map(|(widget, _)| widget.metric())
            .collect::<Vec<_>>();
        if self.slots.len() != metrics.len()
            || !self.slots.iter().any(|slot| slot.visible)
            || metrics
                .iter()
                .any(|kind| self.slots.iter().filter(|slot| slot.kind == *kind).count() != 1)
        {
            return Err("Dashboard layout needs each known widget exactly once and at least one shown widget".into());
        }
        Ok(())
    }
    /// The version-1 order and visibility; the attention widgets follow, shown.
    fn upgrade(&self) -> DashboardLayout {
        let mut slots = self
            .slots
            .iter()
            .map(|slot| WidgetSlot {
                widget: DashboardWidget::from_metric(slot.kind),
                visible: slot.visible,
            })
            .collect::<Vec<_>>();
        for widget in [
            DashboardWidget::StalePrs,
            DashboardWidget::UnresolvedThreads,
        ] {
            slots.push(WidgetSlot {
                widget,
                visible: true,
            });
        }
        DashboardLayout { slots }
    }
}

/// The original single default profile (version 1), read only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDashboardV1 {
    kind: String,
    version: u8,
    host: String,
    account_id: u64,
    repositories: Vec<String>,
    window_days: u64,
    layout: LegacyLayoutV1,
}
impl SavedDashboardV1 {
    pub fn scope(&self) -> Result<Scope, String> {
        Scope::github(&self.host, Some(self.account_id)).map_err(|e| e.to_string())
    }
    pub fn repositories(&self) -> &[String] {
        &self.repositories
    }
    pub fn window_days(&self) -> u64 {
        self.window_days
    }
    pub fn validate_for(&self, scope: &Scope, declared: &[String]) -> Result<(), String> {
        self.layout.validate()?;
        if self.kind != "ghtui-dashboard"
            || self.version != 1
            || self.account_id == 0
            || self.host != self.host.to_ascii_lowercase()
            || self.scope()? != *scope
        {
            return Err(
                "Saved dashboard version, original account or bounds are invalid; nothing restored"
                    .into(),
            );
        }
        validate_preferences(&self.repositories, self.window_days, declared)
    }
    /// The version-1 default as the version-2 dashboard named [`DEFAULT_NAME`].
    pub fn upgrade(&self) -> NamedDashboard {
        NamedDashboard {
            name: DEFAULT_NAME.into(),
            repositories: self.repositories.clone(),
            window_days: self.window_days,
            stale_days: DEFAULT_STALE_DAYS,
            layout: self.layout.upgrade(),
        }
    }
}

/// Bounded scope and window checks shared by both versions. With `declared`, every
/// saved repository must still be declared; a saved scope never widens the current one.
fn validate_preferences(
    repositories: &[String],
    window_days: u64,
    declared: &[String],
) -> Result<(), String> {
    if !matches!(window_days, 7 | 30) || repositories.is_empty() || repositories.len() > 64 {
        return Err(
            "Saved dashboard version, original account or bounds are invalid; nothing restored"
                .into(),
        );
    }
    let mut seen = BTreeSet::new();
    for repository in repositories {
        crate::config::validate_repository(repository).map_err(|e| e.to_string())?;
        if repository.len() > 256
            || *repository != repository.to_ascii_lowercase()
            || !seen.insert(repository)
            || !declared
                .iter()
                .any(|name| name.eq_ignore_ascii_case(repository))
        {
            return Err("Saved scope outside the current declared set, excluded or duplicated; nothing restored".into());
        }
    }
    Ok(())
}

/// A user-chosen dashboard name: trimmed, 1..=40 printable characters.
pub fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name the dashboard first; nothing saved".into());
    }
    if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(format!(
            "Dashboard names are 1..{MAX_NAME_CHARS} printable characters; nothing saved"
        ));
    }
    Ok(name.to_owned())
}

/// One named dashboard: preferences only, never metrics, actors or permission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedDashboard {
    name: String,
    repositories: Vec<String>,
    window_days: u64,
    stale_days: u64,
    layout: DashboardLayout,
}
impl NamedDashboard {
    pub fn capture(
        workspace: &crate::metrics_workspace::MetricsWorkspace,
        name: &str,
    ) -> Result<Self, String> {
        let window = workspace.snapshot.window;
        let span = window
            .end
            .checked_sub(window.start)
            .ok_or("Dashboard window is invalid")?;
        if span % 86_400 != 0 {
            return Err("Saved dashboard needs a whole-day UTC window".into());
        }
        let dashboard = Self {
            name: validate_name(name)?,
            repositories: workspace
                .snapshot
                .scope
                .repositories()
                .iter()
                .cloned()
                .collect(),
            window_days: span / 86_400,
            stale_days: workspace.stale_days,
            layout: workspace.layout.clone(),
        };
        dashboard.validate_for(&workspace.declared_repositories)?;
        Ok(dashboard)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn layout(&self) -> &DashboardLayout {
        &self.layout
    }
    pub fn repositories(&self) -> &[String] {
        &self.repositories
    }
    pub fn window_days(&self) -> u64 {
        self.window_days
    }
    pub fn stale_days(&self) -> u64 {
        self.stale_days
    }
    pub fn validate_for(&self, declared: &[String]) -> Result<(), String> {
        self.layout.validate()?;
        if validate_name(&self.name)? != self.name || !STALE_THRESHOLDS.contains(&self.stale_days) {
            return Err(
                "Saved dashboard version, original account or bounds are invalid; nothing restored"
                    .into(),
            );
        }
        validate_preferences(&self.repositories, self.window_days, declared)
    }
}

/// What the Dashboards list shows of one saved dashboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DashboardEntry {
    pub name: String,
    pub active: bool,
    pub repositories: usize,
    pub window_days: u64,
    pub stale_days: u64,
}

/// Every named dashboard of one account (version 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDashboards {
    kind: String,
    version: u8,
    host: String,
    account_id: u64,
    active: Option<String>,
    dashboards: Vec<NamedDashboard>,
}
impl SavedDashboards {
    /// No dashboards yet for this numeric GitHub account scope.
    pub fn empty(scope: &Scope) -> Result<Self, String> {
        let account_id = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .ok_or("Dashboard preferences need a numeric GitHub account scope")?;
        Ok(Self {
            kind: "ghtui-dashboards".into(),
            version: 2,
            host: scope.host.clone(),
            account_id,
            active: None,
            dashboards: Vec::new(),
        })
    }
    pub fn scope(&self) -> Result<Scope, String> {
        Scope::github(&self.host, Some(self.account_id)).map_err(|e| e.to_string())
    }
    pub fn dashboards(&self) -> &[NamedDashboard] {
        &self.dashboards
    }
    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }
    pub fn entries(&self) -> Vec<DashboardEntry> {
        self.dashboards
            .iter()
            .map(|dashboard| DashboardEntry {
                name: dashboard.name.clone(),
                active: self.active.as_deref() == Some(dashboard.name.as_str()),
                repositories: dashboard.repositories.len(),
                window_days: dashboard.window_days,
                stale_days: dashboard.stale_days,
            })
            .collect()
    }
    fn position(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.dashboards
            .iter()
            .position(|dashboard| dashboard.name.eq_ignore_ascii_case(name))
    }
    /// Structure, account and every dashboard's own bounds. A saved scope is only checked
    /// against the current declared set when that dashboard is restored.
    pub fn validate_for(&self, scope: &Scope) -> Result<(), String> {
        if self.kind != "ghtui-dashboards"
            || self.version != 2
            || self.account_id == 0
            || self.host != self.host.to_ascii_lowercase()
            || self.scope()? != *scope
            || self.dashboards.len() > MAX_DASHBOARDS
        {
            return Err(
                "Saved dashboard version, original account or bounds are invalid; nothing restored"
                    .into(),
            );
        }
        let mut names = BTreeSet::new();
        for dashboard in &self.dashboards {
            dashboard.validate_for(&dashboard.repositories)?;
            if !names.insert(dashboard.name.to_lowercase()) {
                return Err("Saved dashboard names repeat; nothing restored".into());
            }
        }
        if self
            .active
            .as_deref()
            .is_some_and(|active| self.position(active).is_none())
        {
            return Err("Saved active dashboard is missing; nothing restored".into());
        }
        Ok(())
    }
    /// The account's dashboards: the version-2 record, else the version-1 default as
    /// [`DEFAULT_NAME`], else none.
    pub fn load(cache: &Cache, scope: &Scope) -> Result<Option<Self>, String> {
        if let Some(value) = cache
            .saved_view_bounded(scope, PROFILES_KEY, PROFILES_BYTES)
            .map_err(|e| e.to_string())?
        {
            let saved: Self = serde_json::from_value(value)
                .map_err(|e| format!("Saved dashboard is invalid; nothing restored: {e}"))?;
            saved.validate_for(scope)?;
            return Ok(Some(saved));
        }
        let Some(value) = cache
            .saved_view_bounded(scope, PROFILE_KEY, PROFILE_BYTES)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let original: SavedDashboardV1 = serde_json::from_value(value)
            .map_err(|e| format!("Saved dashboard is invalid; nothing restored: {e}"))?;
        original.validate_for(scope, &original.repositories)?;
        let mut saved = Self::empty(scope)?;
        saved.dashboards.push(original.upgrade());
        saved.active = Some(DEFAULT_NAME.into());
        Ok(Some(saved))
    }
    /// The dashboards to change before a write. An unreadable version-1 default is left
    /// untouched and does not block a first version-2 save; an unreadable version-2
    /// record is never overwritten.
    pub fn load_for_write(cache: &Cache, scope: &Scope) -> Result<Self, String> {
        match cache.saved_view_bounded(scope, PROFILES_KEY, PROFILES_BYTES) {
            Ok(Some(_)) | Err(_) => Self::load(cache, scope)
                .and_then(|saved| {
                    saved.ok_or_else(|| "Saved dashboards changed during the write".to_string())
                })
                .map_err(|error| format!("{error} • saved dashboards not overwritten")),
            Ok(None) => match Self::load(cache, scope).ok().flatten() {
                Some(saved) => Ok(saved),
                None => Self::empty(scope),
            },
        }
    }
    /// The named (or else the active) dashboard, validated against the current declared set.
    pub fn restore(
        &self,
        name: Option<&str>,
        declared: &[String],
    ) -> Result<NamedDashboard, String> {
        let name = name
            .or(self.active.as_deref())
            .or_else(|| {
                self.dashboards
                    .first()
                    .map(|dashboard| dashboard.name.as_str())
            })
            .ok_or("No saved dashboard for this account; Save dashboard first")?;
        let index = self
            .position(name)
            .ok_or_else(|| format!("No saved dashboard named {name}; nothing restored"))?;
        let dashboard = self.dashboards[index].clone();
        dashboard.validate_for(declared)?;
        Ok(dashboard)
    }
    /// Replaces the same-named dashboard (or adds it) and makes it active.
    pub fn upsert(&mut self, dashboard: NamedDashboard) -> Result<(), String> {
        dashboard.validate_for(&dashboard.repositories)?;
        match self.position(&dashboard.name) {
            Some(index) => {
                self.active = Some(self.dashboards[index].name.clone());
                self.dashboards[index] = NamedDashboard {
                    name: self.dashboards[index].name.clone(),
                    ..dashboard
                };
            }
            None => {
                if self.dashboards.len() >= MAX_DASHBOARDS {
                    return Err(format!(
                        "At most {MAX_DASHBOARDS} saved dashboards; delete one first"
                    ));
                }
                self.active = Some(dashboard.name.clone());
                self.dashboards.push(dashboard);
            }
        }
        Ok(())
    }
    /// Adds a new dashboard; an existing name is refused, not replaced.
    pub fn create(&mut self, dashboard: NamedDashboard) -> Result<(), String> {
        if self.position(&dashboard.name).is_some() {
            return Err(format!(
                "A dashboard named {} exists; choose another name or Save over it",
                dashboard.name
            ));
        }
        self.upsert(dashboard)
    }
    pub fn activate(&mut self, name: &str) -> Result<(), String> {
        let index = self
            .position(name)
            .ok_or_else(|| format!("No saved dashboard named {name}"))?;
        self.active = Some(self.dashboards[index].name.clone());
        Ok(())
    }
    pub fn rename(&mut self, from: &str, to: &str) -> Result<String, String> {
        let to = validate_name(to)?;
        let index = self
            .position(from)
            .ok_or_else(|| format!("No saved dashboard named {from}; nothing renamed"))?;
        if self.position(&to).is_some_and(|other| other != index) {
            return Err(format!("A dashboard named {to} exists; nothing renamed"));
        }
        let old = std::mem::replace(&mut self.dashboards[index].name, to.clone());
        if self.active.as_deref() == Some(old.as_str()) {
            self.active = Some(to.clone());
        }
        Ok(old)
    }
    pub fn delete(&mut self, name: &str) -> Result<String, String> {
        let index = self
            .position(name)
            .ok_or_else(|| format!("No saved dashboard named {name}; nothing deleted"))?;
        let removed = self.dashboards.remove(index);
        if self.active.as_deref() == Some(removed.name.as_str()) {
            self.active = None;
        }
        Ok(removed.name)
    }
    pub fn save(&self, cache: &Cache) -> Result<(), String> {
        let scope = self.scope()?;
        self.validate_for(&scope)?;
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > PROFILES_BYTES {
            return Err("Dashboard preferences exceed 128KiB; nothing saved".into());
        }
        cache
            .save_view(&scope, PROFILES_KEY, &value)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "dashboard_profile_v1_tests.rs"]
mod tests;
