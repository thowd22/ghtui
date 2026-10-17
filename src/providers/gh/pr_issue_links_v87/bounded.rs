//! Bounded duplicate-rejecting JSON, plus explicit simultaneous lifetimes.
//! The transport reservation is not a tree bound: every key, string, Value,
//! array capacity and map entry is separately debited before it can grow.
use super::*;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

pub(in crate::providers::gh) struct IssueLinkHeapV87 {
    bytes: usize,
    inherited: usize,
    next: u64,
    active: Option<u64>,
}
pub(in crate::providers::gh) struct IssueDocumentReservationV87 {
    id: u64,
    cap: usize,
    before: usize,
}
pub(in crate::providers::gh) struct IssueDocumentV87 {
    id: u64,
    debit: usize,
    value: Value,
}
impl IssueDocumentV87 {
    pub(in crate::providers::gh) fn value(&self) -> &Value {
        &self.value
    }
}
impl IssueLinkHeapV87 {
    pub(in crate::providers::gh) fn detail_pages_v87(
        &mut self,
        reservation: IssueDocumentReservationV87,
        output: crate::providers::runner::CommandOutput,
    ) -> GithubResult<Vec<Value>> {
        let doc = self.decode(reservation, &output.stdout)?;
        let pages = doc
            .value
            .as_array()
            .filter(|pages| !pages.is_empty())
            .ok_or_else(|| refused(GithubErrorKind::Json))?;
        let rows = pages
            .iter()
            .try_fold(0usize, |sum, page| {
                page.as_array().and_then(|rows| sum.checked_add(rows.len()))
            })
            .filter(|rows| *rows <= 10_000)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        let scratch = 3 * PAIR_CAP_V87;
        let tree = doc
            .debit
            .checked_sub(scratch)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        // Raw tree is already charged. Two further tree equivalents cover the
        // retained typed DTO conversions while the last raw page still lives;
        // row terms precede flattening/geometric Vec relocation and all three
        // borrowed identity sets in the unchanged fixed-list validator.
        self.charge(
            tree.checked_mul(2)
                .and_then(|n| {
                    n.checked_add(rows.checked_mul(384 + 2 * std::mem::size_of::<Value>())?)
                })
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
        )?;
        if self.active != Some(doc.id) {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let rows = GithubProvider::api_list_pages_v19(doc.value, None)?;
        // The output/UTF8/serde scratch is DEAD here. Raw row trees and every
        // mapping/identity/vector charge remain inherited by the whole phase.
        drop(output);
        self.bytes = self
            .bytes
            .checked_sub(scratch)
            .filter(|n| *n >= self.inherited)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        self.active = None;
        Ok(rows)
    }
    pub(in crate::providers::gh) fn new(inherited: usize) -> GithubResult<Self> {
        if inherited > MODEL_CAP_V87 {
            return Err(refused(GithubErrorKind::Transport));
        }
        Ok(Self {
            bytes: inherited,
            inherited,
            next: 0,
            active: None,
        })
    }
    pub(in crate::providers::gh) fn charge(&mut self, bytes: usize) -> GithubResult<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= MODEL_CAP_V87)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(())
    }
    pub(in crate::providers::gh) fn retained(&self) -> GithubResult<usize> {
        if self.active.is_some() {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        Ok(self.bytes)
    }
    pub(in crate::providers::gh) fn copy(
        &mut self,
        value: &str,
        max: usize,
    ) -> GithubResult<String> {
        if value.len() > max {
            return Err(refused(GithubErrorKind::Transport));
        }
        // String header + requested bytes + same-sized relocation overlap.
        // This debit remains for the whole phase, including after later moves.
        self.charge(
            std::mem::size_of::<String>()
                .checked_add(
                    value
                        .len()
                        .checked_mul(2)
                        .ok_or_else(|| refused(GithubErrorKind::Transport))?,
                )
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
        )?;
        let owned = value.to_owned();
        if owned.capacity() > value.len().saturating_mul(2) {
            return Err(refused(GithubErrorKind::Transport));
        }
        Ok(owned)
    }
    pub(in crate::providers::gh) fn reserve_document(
        &mut self,
        cap: usize,
    ) -> GithubResult<IssueDocumentReservationV87> {
        if self.active.is_some() || !matches!(cap, SCALAR_CAP_V87 | PAIR_CAP_V87) {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let before = self.bytes;
        // Raw stdout capacity + UTF8/escaped-string parser scratch + retained
        // deserializer scratch coexist. Tree/model allocations are EXTRA below.
        self.charge(
            cap.checked_mul(3)
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
        )?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        self.active = Some(self.next);
        Ok(IssueDocumentReservationV87 {
            id: self.next,
            cap,
            before,
        })
    }
    pub(in crate::providers::gh) fn decode(
        &mut self,
        reservation: IssueDocumentReservationV87,
        bytes: &[u8],
    ) -> GithubResult<IssueDocumentV87> {
        if self.active != Some(reservation.id) || bytes.len() > reservation.cap {
            return Err(refused(GithubErrorKind::Transport));
        }
        let mut de = serde_json::Deserializer::from_slice(bytes);
        let mut nodes = 0usize;
        let value = Seed {
            heap: self,
            depth: 0,
            nodes: &mut nodes,
        }
        .deserialize(&mut de)
        .map_err(|_| refused(GithubErrorKind::Json))?;
        de.end().map_err(|_| refused(GithubErrorKind::Json))?;
        let debit = self
            .bytes
            .checked_sub(reservation.before)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(IssueDocumentV87 {
            id: reservation.id,
            debit,
            value,
        })
    }
    pub(in crate::providers::gh) fn release_document(
        &mut self,
        doc: IssueDocumentV87,
    ) -> GithubResult<()> {
        if self.active != Some(doc.id) {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let debit = doc.debit;
        // Actual raw Value is dropped FIRST. Only its known capture/tree debit
        // is released, not inherited or persistent typed rows added afterwards.
        drop(doc);
        self.bytes = self
            .bytes
            .checked_sub(debit)
            .filter(|n| *n >= self.inherited)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        self.active = None;
        Ok(())
    }
}

struct Seed<'a> {
    heap: &'a mut IssueLinkHeapV87,
    depth: usize,
    nodes: &'a mut usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<Value, D::Error> {
        if self.depth > 24 || *self.nodes >= 32_768 {
            return Err(serde::de::Error::custom(
                "bounded Issue-link JSON depth/nodes",
            ));
        }
        *self.nodes += 1;
        self.heap
            .charge(std::mem::size_of::<Value>() + 128)
            .map_err(serde::de::Error::custom)?;
        de.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded duplicate-free Issue-link JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite"))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(self.heap.copy(v, 65_536).map_err(E::custom)?))
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Value, E> {
        if v.len() > 65_536 || v.capacity() > 128 * 1024 {
            return Err(E::custom("bounded string"));
        }
        // Escaped JSON's temporary owned buffer was already covered by the
        // pre-read scratch. Transfer is additionally charged as retained tree.
        self.heap
            .charge(v.capacity() + std::mem::size_of::<String>())
            .map_err(E::custom)?;
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        loop {
            // reserve_exact grows only after a finite debit for BOTH requested
            // and previous capacity; no trust in a server size hint.
            if values.len() == values.capacity() {
                let next = values
                    .capacity()
                    .max(2)
                    .checked_mul(2)
                    .filter(|n| *n <= 4096)
                    .ok_or_else(|| serde::de::Error::custom("bounded array"))?;
                self.heap
                    .charge(
                        next.checked_mul(std::mem::size_of::<Value>() * 2)
                            .ok_or_else(|| serde::de::Error::custom("array overflow"))?,
                    )
                    .map_err(serde::de::Error::custom)?;
                values.reserve_exact(next - values.len());
                if values.capacity() > next * 2 {
                    return Err(serde::de::Error::custom("array capacity"));
                }
            }
            let next = seq.next_element_seed(Seed {
                heap: self.heap,
                depth: self.depth + 1,
                nodes: self.nodes,
            })?;
            match next {
                Some(value) => values.push(value),
                None => break,
            }
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key_seed(KeySeed { heap: self.heap })? {
            if values.len() >= 2048 || values.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate/bounded object key"));
            }
            // Conservative full map-node/key/value/Set bookkeeping term; not
            // merely the raw bytes. It precedes insertion and next value parse.
            self.heap
                .charge(512 + std::mem::size_of::<(String, Value)>())
                .map_err(serde::de::Error::custom)?;
            let value = map.next_value_seed(Seed {
                heap: self.heap,
                depth: self.depth + 1,
                nodes: self.nodes,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
struct KeySeed<'a> {
    heap: &'a mut IssueLinkHeapV87,
}
impl<'de> DeserializeSeed<'de> for KeySeed<'_> {
    type Value = String;
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<String, D::Error> {
        de.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for KeySeed<'_> {
    type Value = String;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded object key")
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<String, E> {
        self.heap.copy(v, 1024).map_err(E::custom)
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<String, E> {
        if v.len() > 1024 || v.capacity() > 2048 {
            return Err(E::custom("bounded key"));
        }
        self.heap
            .charge(v.capacity() + std::mem::size_of::<String>())
            .map_err(E::custom)?;
        Ok(v)
    }
}
