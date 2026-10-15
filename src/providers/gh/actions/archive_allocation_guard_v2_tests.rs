//! SOURCE ONLY: pinned-parser allocation admission and actual shared ZIP/CRC.
use super::*;
use crate::scoped_actions_resources_test_support_v2 as r;
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba([40, 90, 170, 255]),
    ))
    .write_to(&mut out, image::ImageFormat::Png)
    .unwrap();
    out.into_inner()
}
#[test]
fn scoped_actions_resources_v2_image_uses_same_decoder_narrow_dimension_legacy_unchanged() {
    let bytes = png(1025, 1);
    assert!(crate::identity::ImageDisplay::decode_actions_preview_v2(&bytes).is_err());
    let legacy = crate::identity::ImageDisplay::decode(&bytes).unwrap();
    assert_eq!(legacy.original_width, 1025);
}
#[test]
fn scoped_actions_resources_v2_image_real1024_raster_bounded_thumbnail_preserves_color() {
    let image = crate::identity::ImageDisplay::decode_actions_preview_v2(&png(1024, 1024)).unwrap();
    assert_eq!((image.original_width, image.original_height), (1024, 1024));
    assert!(image.raster.width <= 128 && image.raster.height <= 128);
    assert!(image.raster.pixels.contains(&[40, 90, 170, 255]));
}
fn footer(bytes: &[u8]) -> usize {
    bytes.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap()
}
fn central(bytes: &[u8]) -> usize {
    bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap()
}
#[test]
fn scoped_actions_resources_v2_zip_admitted_profile_uses_same_parser_and_preview_crc() {
    let bytes = r::zip("nested/界.txt", b"REAL_SHARED_ZIP_CRC");
    check(&bytes, &Default::default()).unwrap();
    let archive = ArtifactArchive::inspect(bytes, None, r::limits()).unwrap();
    assert_eq!(
        archive.preview("nested/界.txt").unwrap(),
        b"REAL_SHARED_ZIP_CRC"
    );
}
#[test]
fn scoped_actions_resources_v2_zip_hostile_advertised_count_refused_before_parser_reserve() {
    let mut bytes = r::zip("real.txt", b"x");
    let at = footer(&bytes);
    bytes[at + 8..at + 12].copy_from_slice(&[0xff; 4]);
    assert!(check(&bytes, &Default::default()).is_err());
}
#[test]
fn scoped_actions_resources_v2_zip64_footer_or_locator_refused_before_parser() {
    let mut bytes = r::zip("real.txt", b"x");
    bytes.extend_from_slice(b"PK\x06\x07");
    assert!(check(&bytes, &Default::default()).is_err());
}
#[test]
fn scoped_actions_resources_v2_zip_central_extent_mismatch_refused_before_parser() {
    let mut bytes = r::zip("real.txt", b"x");
    let at = footer(&bytes);
    bytes[at + 12..at + 16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(check(&bytes, &Default::default()).is_err());
}
#[test]
fn scoped_actions_resources_v2_zip_alternate_footer_in_payload_is_explicit_unsupported() {
    let bytes = r::zip("real.txt", b"before PK\x05\x06 after");
    let error = check(&bytes, &Default::default()).err().unwrap();
    assert!(error.message.contains("alternate EOCD"));
}
#[test]
fn scoped_actions_resources_v2_zip_truncated_central_record_refuses_before_parser() {
    let mut bytes = r::zip("real.txt", b"x");
    let at = central(&bytes);
    bytes[at + 28..at + 30].copy_from_slice(&4096u16.to_le_bytes());
    assert!(check(&bytes, &Default::default()).is_err());
}
#[test]
fn scoped_actions_resources_v2_zip_original_cancel_is_checked_without_parser_or_preview() {
    let bytes = r::zip("real.txt", b"x");
    let token = CancellationToken::default();
    token.cancel();
    assert!(check(&bytes, &token).is_err());
}
#[test]
fn scoped_actions_resources_v2_zip_capacity_not_only_length_is_admitted() {
    let mut bytes = r::zip("real.txt", b"x");
    bytes.reserve_exact(9 * 1024 * 1024);
    assert!(bytes.len() < 4096);
    assert!(check(&bytes, &Default::default()).is_err());
}
#[test]
fn scoped_actions_resources_v2_zip_corrupt_stored_bytes_fails_shared_crc_not_fabricated_preview() {
    let mut bytes = r::zip("real.txt", b"CRC_ORIGINAL");
    let at = bytes
        .windows(12)
        .position(|w| w == b"CRC_ORIGINAL")
        .unwrap();
    bytes[at] = b'X';
    check(&bytes, &Default::default()).unwrap();
    let archive = ArtifactArchive::inspect(bytes, None, r::limits()).unwrap();
    assert!(archive.preview("real.txt").is_err());
}
