//! Bounded discovery request intent. Never permission, coverage or
//! a source factory receipt. Editing/previewing this model performs no I/O.
use crate::providers::gh::RepositoryCatalogueLimits;
use std::time::Duration;

pub const DISCOVERY_FIELD_COUNT: usize = 5;
pub const DISCOVERY_FIELD_BYTES: usize = 6;
const MIB: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryLimitFieldV1 {
    Pages,
    RawRows,
    MetadataMib,
    CaptureMib,
    DeadlineSeconds,
}
impl DiscoveryLimitFieldV1 {
    pub const ALL: [Self; DISCOVERY_FIELD_COUNT] = [
        Self::Pages,
        Self::RawRows,
        Self::MetadataMib,
        Self::CaptureMib,
        Self::DeadlineSeconds,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Pages => "Pages (1..1000): ",
            Self::RawRows => "Raw rows (100..100000): ",
            Self::MetadataMib => "Metadata MiB (1..32): ",
            Self::CaptureMib => "Raw capture MiB (1..64): ",
            Self::DeadlineSeconds => "Deadline seconds (1..300): ",
        }
    }
}

/// Private bounded fields; this is local READ intent, not authority. Defaults
/// preserve existing20/page100/2000/8MiB/16MiB/30s discovery behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogueDiscoveryBudgetV1 {
    values: [usize; DISCOVERY_FIELD_COUNT],
}
impl Default for CatalogueDiscoveryBudgetV1 {
    fn default() -> Self {
        Self {
            values: [20, 2000, 8, 16, 30],
        }
    }
}
impl CatalogueDiscoveryBudgetV1 {
    /// Bound every input BEFORE parsing/owned copies; no clamp or guessed value.
    pub fn from_fields(fields: [&str; DISCOVERY_FIELD_COUNT]) -> Result<Self, &'static str> {
        let mut values = [0; DISCOVERY_FIELD_COUNT];
        for (input, value) in fields.into_iter().zip(&mut values) {
            if input.is_empty()
                || input.len() > DISCOVERY_FIELD_BYTES
                || !input.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("Use 1..6 ASCII decimal digits per field; no request sent");
            }
            *value = input
                .parse()
                .map_err(|_| "Discovery numeric bound is invalid; no request sent")?;
        }
        if !(1..=1000).contains(&values[0])
            || !(100..=100000).contains(&values[1])
            || !(1..=32).contains(&values[2])
            || !(1..=64).contains(&values[3])
            || !(1..=300).contains(&values[4])
        {
            return Err(
                "One or more discovery bounds are outside the displayed ranges; no request sent",
            );
        }
        Ok(Self { values })
    }
    pub fn preset(index: usize) -> Result<Self, &'static str> {
        Ok(match index {
            0 => Self::default(),
            1 => Self {
                values: [100, 10000, 16, 32, 120],
            },
            2 => Self {
                values: [1000, 100000, 32, 64, 300],
            },
            _ => return Err("Unknown discovery limits preset; current intent unchanged"),
        })
    }
    /// All larger bounds are EXPLICIT intent, not expected loaded rows or time.
    pub fn suggested_larger(self) -> Self {
        for index in [1, 2] {
            let next = Self::preset(index).expect("fixed bounded preset");
            if self
                .values
                .iter()
                .zip(next.values)
                .any(|(old, new)| *old < new)
            {
                let values = std::array::from_fn(|i| self.values[i].max(next.values[i]));
                return Self { values };
            }
        }
        self
    }
    pub fn fields(self) -> [String; DISCOVERY_FIELD_COUNT] {
        self.values.map(|value| value.to_string())
    }
    pub fn limits(self) -> RepositoryCatalogueLimits {
        RepositoryCatalogueLimits {
            max_pages: self.values[0],
            page_size: 100,
            max_rows: self.values[1],
            max_document_bytes: 128 * 1024,
            max_metadata_bytes: self.values[2] * MIB,
            max_response_bytes: self.values[3] * MIB,
            max_duration: Duration::from_secs(self.values[4] as u64),
        }
    }
    pub fn summary(self) -> String {
        format!(
            "Request bounds: {} pages / {} raw rows / {} MiB metadata / {} MiB raw / {} s",
            self.values[0], self.values[1], self.values[2], self.values[3], self.values[4]
        )
    }
}

#[cfg(test)]
#[path = "repository_catalogue_read_limits_v1_tests.rs"]
mod tests;
