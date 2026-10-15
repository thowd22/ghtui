//! Untrusted archive preview preparation; decoding never happens during rendering.
use super::*;
use crate::{identity::ImageDisplay, providers::runner::sanitize_terminal};

pub(super) struct ArtifactContent {
    pub text: String,
    pub image: Option<Arc<ImageDisplay>>,
}
pub(super) fn prepare_content(
    archive: &ArtifactArchive,
    path: &str,
    cancel: &CancellationToken,
) -> GithubResult<ArtifactContent> {
    prepare_content_bounded_v2(archive, path, cancel, false)
}
pub(super) fn prepare_scoped_content_v2(
    archive: &ArtifactArchive,
    path: &str,
    cancel: &CancellationToken,
) -> GithubResult<ArtifactContent> {
    prepare_content_bounded_v2(archive, path, cancel, true)
}
fn prepare_content_bounded_v2(
    archive: &ArtifactArchive,
    path: &str,
    cancel: &CancellationToken,
    scoped: bool,
) -> GithubResult<ArtifactContent> {
    if cancel.is_cancelled() {
        return Err(plain_error("Artifact preview cancelled"));
    }
    let entry = archive
        .entries
        .iter()
        .find(|entry| entry.path == path && !entry.directory)
        .ok_or_else(|| plain_error("Choose an existing regular archive file"))?;
    let bytes = archive.preview(path)?;
    let truncated = (bytes.len() as u64) < entry.expanded_bytes;
    let mut heading = format!(
        "Preview {} of {} bytes{} • {}",
        bytes.len(),
        entry.expanded_bytes,
        if truncated {
            " [TRUNCATED]"
        } else {
            " [complete file]"
        },
        sanitize_terminal(path)
    );
    let looks_image = bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.starts_with(b"\xff\xd8\xff");
    if looks_image && !truncated {
        let image = if scoped {
            ImageDisplay::decode_actions_preview_v2(&bytes)
        } else {
            ImageDisplay::decode(&bytes)
        };
        match image {
            Ok(image) => {
                heading.push_str(&format!(
                    "\nBounded {} image • {}×{} original pixels • thumbnail {}×{}",
                    image.format,
                    image.original_width,
                    image.original_height,
                    image.raster.width,
                    image.raster.height
                ));
                if cancel.is_cancelled() {
                    return Err(plain_error("Artifact image preview cancelled"));
                }
                return Ok(ArtifactContent {
                    text: heading,
                    image: Some(Arc::new(image)),
                });
            }
            Err(error) => heading.push_str(&format!(
                "\nImage decode unavailable: {}",
                sanitize_terminal(&error.to_string())
            )),
        }
    } else if looks_image {
        heading.push_str("\nImage decode skipped: the bounded byte preview is truncated.");
    }
    if !looks_image && !bytes.contains(&0) {
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => Some(text),
            Err(error) if truncated && error.error_len().is_none() => {
                std::str::from_utf8(&bytes[..error.valid_up_to()]).ok()
            }
            _ => None,
        };
        if let Some(text) = text {
            let sanitized = sanitize_terminal(text);
            let mut lines = sanitized.lines();
            heading.push_str("\n--- Bounded text preview ---\n");
            for line in lines.by_ref().take(4096) {
                heading.push_str(line);
                heading.push('\n');
            }
            if lines.next().is_some() {
                heading.push_str(
                    "[Additional preview lines omitted; export retains original bytes.]\n",
                );
            }
            return Ok(ArtifactContent {
                text: heading,
                image: None,
            });
        }
    }
    heading.push_str("\n--- Binary bytes (first 256 bytes; never executed) ---\n");
    for (index, chunk) in bytes
        .iter()
        .take(256)
        .copied()
        .collect::<Vec<_>>()
        .chunks(16)
        .enumerate()
    {
        heading.push_str(&format!(
            "{:08x}  {}\n",
            index * 16,
            chunk
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    Ok(ArtifactContent {
        text: heading,
        image: None,
    })
}
