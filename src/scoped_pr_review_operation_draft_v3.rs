//! Bounded ordinary body restoration. Publication references are history ONLY.
//! No anchor, numeric source, capability, phase or approval is deserialized.
use super::*;
use serde::Serialize;
use std::io::Write;

struct Bounded {
    count: usize,
    output: Option<Vec<u8>>,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.count = self
            .count
            .checked_add(bytes.len())
            .filter(|count| *count <= REVIEW_DRAFT_BYTES_V2)
            .ok_or_else(|| std::io::Error::other("Actual operation draft exceeds128KiB"))?;
        if let Some(output) = &mut self.output {
            output.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Worker-only counting preflight before any journal begin or extra transport.
/// The node/head/digest are the ACTUAL prepared original write metadata. Only
/// the unknown positive SQLite attempt ID uses its maximum encoded width. This
/// is an envelope-size witness, never a constructed publication reference.
pub(crate) fn preflight_publication_envelope(
    body: &str,
    pr_node_id: &str,
    observed_head: &str,
    payload_sha256: &str,
) -> Result<(), String> {
    if body.len() > 65_536
        || pr_node_id.is_empty()
        || pr_node_id.len() > 1024
        || !matches!(observed_head.len(), 40 | 64)
        || payload_sha256.len() != 64
        || !observed_head.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !payload_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Actual original operation reference metadata exceeds bound".into());
    }
    #[derive(Serialize)]
    struct ReferenceSize<'a> {
        attempt_id: i64,
        pr_node_id: &'a str,
        observed_head: &'a str,
        payload_sha256: &'a str,
    }
    #[derive(Serialize)]
    struct EnvelopeSize<'a> {
        version: u8,
        body: &'a str,
        publication: ReferenceSize<'a>,
    }
    let value = EnvelopeSize {
        version: 3,
        body,
        publication: ReferenceSize {
            attempt_id: i64::MAX,
            pr_node_id,
            observed_head,
            payload_sha256,
        },
    };
    let mut count = Bounded {
        count: 0,
        output: None,
    };
    serde_json::to_writer(&mut count, &value).map_err(|_| {
        "Actual escaped operation body and original reference exceed128KiB before allocation".into()
    })
}

pub fn encode(body: &str, publication: Option<&PrPublicationReference>) -> Result<String, String> {
    if body.len() > 65_536 {
        return Err("Review operation body exceeds64KiB".into());
    }
    #[derive(Serialize)]
    struct Envelope<'a> {
        version: u8,
        body: &'a str,
        publication: Option<&'a PrPublicationReference>,
    }
    let value = Envelope {
        version: 3,
        body,
        publication,
    };
    let mut count = Bounded {
        count: 0,
        output: None,
    };
    serde_json::to_writer(&mut count, &value)
        .map_err(|_| "Actual escaped operation draft exceeds128KiB before allocation")?;
    let mut output = Bounded {
        count: 0,
        output: Some(Vec::with_capacity(count.count)),
    };
    serde_json::to_writer(&mut output, &value)
        .map_err(|_| "Original operation draft encoding unavailable")?;
    String::from_utf8(output.output.unwrap_or_default())
        .map_err(|_| "Original operation draft encoding unavailable".into())
}
pub fn decode(encoded: &str) -> Result<(String, Option<PrPublicationReference>), String> {
    if encoded.len() > REVIEW_DRAFT_BYTES_V2 {
        return Err("Saved operation draft exceeds128KiB before parse".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(encoded).map_err(|_| "Original operation draft malformed")?;
    let object = value
        .as_object()
        .filter(|object| {
            object.len() == 3
                && ["version", "body", "publication"]
                    .iter()
                    .all(|key| object.contains_key(*key))
        })
        .ok_or("Unknown/missing operation draft fields; no partial restore")?;
    let body = object["body"]
        .as_str()
        .filter(|body| body.len() <= 65_536)
        .ok_or("Original operation body unavailable")?;
    if object["version"].as_u64() != Some(3) {
        return Err("Unsupported original operation draft version".into());
    }
    if !object["publication"].is_null() {
        let reference = object["publication"]
            .as_object()
            .filter(|reference| {
                reference.len() == 4
                    && reference
                        .get("attempt_id")
                        .and_then(serde_json::Value::as_i64)
                        .is_some_and(|id| id > 0)
            })
            .ok_or("Original operation reference malformed")?;
        for (field, sizes) in [
            ("pr_node_id", &[1024usize][..]),
            ("observed_head", &[40usize, 64][..]),
            ("payload_sha256", &[64usize][..]),
        ] {
            reference
                .get(field)
                .and_then(serde_json::Value::as_str)
                .filter(|text| {
                    if field == "pr_node_id" {
                        !text.is_empty()
                            && text.len() <= sizes[0]
                            && !text.chars().any(char::is_control)
                    } else {
                        sizes.contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
                    }
                })
                .ok_or("Original operation reference exceeds bound before clone")?;
        }
    }
    let reference = serde_json::from_value(object["publication"].clone())
        .map_err(|_| "Original operation reference malformed")?;
    Ok((body.into(), reference))
}
