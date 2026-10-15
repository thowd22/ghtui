//! Account-scoped dashboard preferences, never cached evidence or access authority.
use crate::{
    metrics::MetricKind,
    storage::{Cache, Scope},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PROFILE_KEY: &str = "ghtui/dashboard/default/v1";
pub const PROFILE_BYTES: usize = 32 * 1024;
pub const WIDGETS: [(MetricKind, &str); 8] = [
    (MetricKind::OpenPrs, "Loaded open PRs"),
    (MetricKind::NeedsMyReview, "Needs my review"),
    (MetricKind::OpenAge, "Mean open PR age"),
    (MetricKind::ReviewWait, "Mean requested-review wait"),
    (
        MetricKind::FirstReview,
        "Mean first submitted review latency",
    ),
    (MetricKind::MergeLatency, "Mean merge latency"),
    (MetricKind::Checks, "Observed exact-head check health"),
    (
        MetricKind::Workflows,
        "Latest observed workflow-attempt health",
    ),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetSlotV1 {
    pub kind: MetricKind,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardLayoutV1 {
    slots: Vec<WidgetSlotV1>,
}
impl Default for DashboardLayoutV1 {
    fn default() -> Self {
        Self {
            slots: WIDGETS
                .iter()
                .map(|(kind, _)| WidgetSlotV1 {
                    kind: *kind,
                    visible: true,
                })
                .collect(),
        }
    }
}
impl DashboardLayoutV1 {
    pub fn slots(&self) -> &[WidgetSlotV1] {
        &self.slots
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.slots.len() != WIDGETS.len()
            || !self.slots.iter().any(|slot| slot.visible)
            || WIDGETS
                .iter()
                .any(|(kind, _)| self.slots.iter().filter(|slot| slot.kind == *kind).count() != 1)
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
    pub fn shown(&self) -> impl Iterator<Item = MetricKind> + '_ {
        self.slots
            .iter()
            .filter(|slot| slot.visible)
            .map(|slot| slot.kind)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDashboardV1 {
    kind: String,
    version: u8,
    host: String,
    account_id: u64,
    repositories: Vec<String>,
    window_days: u64,
    layout: DashboardLayoutV1,
}
impl SavedDashboardV1 {
    pub fn capture(workspace: &crate::metrics_workspace::MetricsWorkspace) -> Result<Self, String> {
        let window = workspace.snapshot.window;
        let span = window
            .end
            .checked_sub(window.start)
            .ok_or("Dashboard window is invalid")?;
        if span % 86_400 != 0 {
            return Err("Saved dashboard needs a whole-day UTC window".into());
        }
        let profile = Self {
            kind: "ghtui-dashboard".into(),
            version: 1,
            host: workspace.snapshot.scope.host().into(),
            account_id: workspace.snapshot.scope.account_id(),
            repositories: workspace
                .snapshot
                .scope
                .repositories()
                .iter()
                .cloned()
                .collect(),
            window_days: span / 86_400,
            layout: workspace.layout.clone(),
        };
        profile.validate_for(&profile.scope()?, &workspace.declared_repositories)?;
        Ok(profile)
    }
    pub fn scope(&self) -> Result<Scope, String> {
        Scope::github(&self.host, Some(self.account_id)).map_err(|e| e.to_string())
    }
    pub fn layout(&self) -> &DashboardLayoutV1 {
        &self.layout
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
            || !matches!(self.window_days, 7 | 30)
            || self.repositories.is_empty()
            || self.repositories.len() > 64
        {
            return Err(
                "Saved dashboard version, original account or bounds are invalid; nothing restored"
                    .into(),
            );
        }
        let mut seen = BTreeSet::new();
        for repository in &self.repositories {
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
    pub fn save(&self, cache: &Cache) -> Result<(), String> {
        let scope = self.scope()?;
        self.validate_for(&scope, &self.repositories)?;
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > PROFILE_BYTES {
            return Err("Dashboard preferences exceed32KiB; nothing saved".into());
        }
        cache
            .save_view(&scope, PROFILE_KEY, &value)
            .map_err(|e| e.to_string())
    }
    pub fn load(cache: &Cache, scope: &Scope, declared: &[String]) -> Result<Self, String> {
        let value = cache
            .saved_view_bounded(scope, PROFILE_KEY, PROFILE_BYTES)
            .map_err(|e| e.to_string())?
            .ok_or("No default dashboard saved for this account; Save dashboard first")?;
        let profile: Self = serde_json::from_value(value)
            .map_err(|e| format!("Saved dashboard is invalid; nothing restored: {e}"))?;
        profile.validate_for(scope, declared)?;
        Ok(profile)
    }
}

#[cfg(test)]
#[path = "dashboard_profile_v1_tests.rs"]
mod tests;
