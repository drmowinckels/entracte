//! Plugin image assets (#156): a content plugin may ship images and reference
//! them from routine steps, so a guided break can show what a stretch looks
//! like rather than only describing it.
//!
//! Assets travel **inline** in the manifest as base64, exactly like a
//! detector's `module_base64` — so a plugin is still one signed file. The
//! signature binds each asset by the `sha256` declared alongside it (the
//! `data_base64` blob is excluded from the signing payload; see
//! [`super::signature::signing_payload`]), and [`validate_asset`] independently
//! checks the bytes hash to that declared value. A tampered blob therefore
//! fails either the signature (if the hash was changed) or the hash check (if
//! only the bytes were swapped).
//!
//! Everything here is pure — no I/O — and the format/dimension sniffing reads
//! only header fields (it never decodes pixels), so a hostile file cannot turn
//! validation itself into a decompression bomb. The pixel-count cap then bounds
//! what the overlay will later decode.

use base64::prelude::{Engine, BASE64_STANDARD};
use serde::{Deserialize, Serialize};

use super::signature::sha256;

/// Most images a pack ever needs; bounds the manifest and the install dialog.
pub const MAX_ASSETS: usize = 64;
/// Per-asset decoded-byte cap. Generous for a UI illustration, small enough
/// that 64 of them stay well under the 8 MiB manifest cap.
pub const MAX_ASSET_BYTES: usize = 512 * 1024;
/// Decode-time pixel cap (width × height). The decompression-bomb guard: a tiny
/// compressed file can claim enormous dimensions, so we reject on the declared
/// header size before anything decodes it.
pub const MAX_IMAGE_PIXELS: u64 = 4_000_000;
/// Tighter byte cap for audio cues. A cue is a short sound, not a track; the
/// size bounds the playback length without parsing every container's duration.
pub const MAX_SOUND_BYTES: usize = 256 * 1024;
const MAX_ASSET_ID_LEN: usize = 128;

/// One inline image declared in a manifest. `data_base64` is excluded from the
/// signing payload; the signature binds the image through `sha256`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestAsset {
    /// Pack-local identifier a routine step references. Filename-safe so it can
    /// name the on-disk sidecar without traversal: `[a-z0-9._-]`.
    pub id: String,
    /// Lowercase hex SHA-256 of the decoded image bytes. Part of the signed
    /// canonical manifest (only `data_base64` is stripped before signing).
    pub sha256: String,
    /// The image itself, base64 (standard alphabet). Stripped from the signing
    /// payload — bound by `sha256` instead.
    #[serde(default)]
    pub data_base64: String,
}

/// The image formats a plugin may ship. Chosen for small UI illustrations and
/// simple animations; each has a header we can read dimensions from without
/// decoding pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Gif,
    Webp,
}

impl ImageFormat {
    /// File extension used for the on-disk sidecar.
    pub fn ext(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Gif => "gif",
            ImageFormat::Webp => "webp",
        }
    }
}

/// The audio formats a plugin may ship for break/routine cues. Detected by
/// container magic bytes; the byte cap bounds their length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Ogg,
    Wav,
    Mp3,
}

impl AudioFormat {
    pub fn ext(self) -> &'static str {
        match self {
            AudioFormat::Ogg => "ogg",
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
        }
    }
}

/// What a validated asset turned out to be. Routine images reference an
/// `Image`; sound cues reference an `Audio`. The kind lets the manifest check
/// that each reference points at the right sort of asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Image(ImageFormat),
    Audio(AudioFormat),
}

impl AssetKind {
    /// File extension for the on-disk sidecar.
    pub fn ext(self) -> &'static str {
        match self {
            AssetKind::Image(f) => f.ext(),
            AssetKind::Audio(f) => f.ext(),
        }
    }

    pub fn is_audio(self) -> bool {
        matches!(self, AssetKind::Audio(_))
    }
}

fn u16le(b: &[u8]) -> u64 {
    b[0] as u64 | (b[1] as u64) << 8
}

fn u24le(b: &[u8]) -> u64 {
    b[0] as u64 | (b[1] as u64) << 8 | (b[2] as u64) << 16
}

fn u32le(b: &[u8]) -> u64 {
    b[0] as u64 | (b[1] as u64) << 8 | (b[2] as u64) << 16 | (b[3] as u64) << 24
}

/// Identify an image's format and pixel dimensions from its header alone.
/// Returns `None` for anything not in the allowlist or with a header too short
/// or malformed to read. Never decodes pixel data.
pub fn sniff(bytes: &[u8]) -> Option<(ImageFormat, u64, u64)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        // IHDR is the first chunk: width/height are big-endian u32 at 16..24.
        let w = u32be(bytes.get(16..20)?);
        let h = u32be(bytes.get(20..24)?);
        return Some((ImageFormat::Png, w, h));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        // Logical-screen width/height are little-endian u16 at 6..10.
        let w = u16le(bytes.get(6..8)?);
        let h = u16le(bytes.get(8..10)?);
        return Some((ImageFormat::Gif, w, h));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return sniff_webp(bytes);
    }
    None
}

fn u32be(b: &[u8]) -> u64 {
    (b[0] as u64) << 24 | (b[1] as u64) << 16 | (b[2] as u64) << 8 | b[3] as u64
}

/// Identify an audio container from its magic bytes. WAV shares the `RIFF`
/// header with WebP — the `WAVE` form type at 8..12 disambiguates.
pub fn sniff_audio(bytes: &[u8]) -> Option<AudioFormat> {
    if bytes.starts_with(b"OggS") {
        return Some(AudioFormat::Ogg);
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        return Some(AudioFormat::Wav);
    }
    // MP3: an ID3v2 tag, or a bare MPEG frame sync (11 set bits: FF Ex/Fx).
    if bytes.starts_with(b"ID3") {
        return Some(AudioFormat::Mp3);
    }
    if let Some(&[b0, b1]) = bytes.get(0..2).and_then(|s| <&[u8; 2]>::try_from(s).ok()) {
        if b0 == 0xff && (b1 & 0xe0) == 0xe0 {
            return Some(AudioFormat::Mp3);
        }
    }
    None
}

/// Dimensions from the three WebP chunk layouts (extended, lossless, lossy).
fn sniff_webp(bytes: &[u8]) -> Option<(ImageFormat, u64, u64)> {
    match bytes.get(12..16)? {
        b"VP8X" => {
            // Canvas size is two 24-bit little-endian values, each minus one.
            let w = u24le(bytes.get(24..27)?) + 1;
            let h = u24le(bytes.get(27..30)?) + 1;
            Some((ImageFormat::Webp, w, h))
        }
        b"VP8L" => {
            // 0x2f signature, then 14-bit width-1 and 14-bit height-1 packed
            // into a little-endian u32.
            if bytes.get(20) != Some(&0x2f) {
                return None;
            }
            let v = u32le(bytes.get(21..25)?);
            let w = (v & 0x3fff) + 1;
            let h = ((v >> 14) & 0x3fff) + 1;
            Some((ImageFormat::Webp, w, h))
        }
        b"VP8 " => {
            // Lossy keyframe: 14-bit width/height little-endian at 26..30,
            // after the 3-byte start code.
            let w = u16le(bytes.get(26..28)?) & 0x3fff;
            let h = u16le(bytes.get(28..30)?) & 0x3fff;
            Some((ImageFormat::Webp, w, h))
        }
        _ => None,
    }
}

/// `true` if `id` is a safe single filename component: non-empty, within the
/// length cap, and only `[a-z0-9._-]` (no path separators, no traversal).
fn is_safe_asset_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ASSET_ID_LEN
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Decode and fully validate one declared asset, returning its decoded bytes
/// and sniffed format on success. Checks, first-error-wins: a filename-safe id,
/// a 64-char lowercase-hex sha256, valid base64, the decoded-byte cap, that the
/// bytes hash to the declared sha256, an allowed format, and the pixel cap.
pub fn validate_asset(asset: &ManifestAsset) -> Result<(Vec<u8>, AssetKind), String> {
    if !is_safe_asset_id(&asset.id) {
        return Err(format!(
            "asset id '{}' must be 1..={MAX_ASSET_ID_LEN} chars of [a-z0-9._-]",
            asset.id
        ));
    }
    if asset.sha256.len() != 64
        || !asset
            .sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(format!(
            "asset '{}' sha256 must be 64 lowercase hex characters",
            asset.id
        ));
    }
    let bytes = BASE64_STANDARD
        .decode(asset.data_base64.as_bytes())
        .map_err(|_| format!("asset '{}' data is not valid base64", asset.id))?;
    if bytes.is_empty() {
        return Err(format!("asset '{}' is empty", asset.id));
    }
    if bytes.len() > MAX_ASSET_BYTES {
        return Err(format!(
            "asset '{}' is {} bytes, over the {MAX_ASSET_BYTES}-byte cap",
            asset.id,
            bytes.len()
        ));
    }
    let actual = hex_lower(&sha256(&bytes));
    if actual != asset.sha256 {
        return Err(format!(
            "asset '{}' bytes do not match its declared sha256",
            asset.id
        ));
    }
    if let Some((format, w, h)) = sniff(&bytes) {
        if w == 0 || h == 0 || w.saturating_mul(h) > MAX_IMAGE_PIXELS {
            return Err(format!(
                "asset '{}' is {w}x{h}, over the {MAX_IMAGE_PIXELS}-pixel cap",
                asset.id
            ));
        }
        return Ok((bytes, AssetKind::Image(format)));
    }
    if let Some(format) = sniff_audio(&bytes) {
        if bytes.len() > MAX_SOUND_BYTES {
            return Err(format!(
                "sound '{}' is {} bytes, over the {MAX_SOUND_BYTES}-byte cap",
                asset.id,
                bytes.len()
            ));
        }
        return Ok((bytes, AssetKind::Audio(format)));
    }
    Err(format!(
        "asset '{}' is not a supported image (png/gif/webp) or sound (ogg/wav/mp3)",
        asset.id
    ))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        v.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]); // bit depth, color type, etc.
        v
    }

    fn gif(w: u16, h: u16) -> Vec<u8> {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v
    }

    fn webp_vp8x(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]); // file size (ignored)
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(b"VP8X");
        v.extend_from_slice(&[0, 0, 0, 0]); // chunk size
        v.extend_from_slice(&[0, 0, 0, 0]); // flags + reserved (offset 20..24)
        let wm = w - 1;
        let hm = h - 1;
        v.extend_from_slice(&wm.to_le_bytes()[..3]); // 24-bit width-1 at 24..27
        v.extend_from_slice(&hm.to_le_bytes()[..3]); // 24-bit height-1 at 27..30
        v
    }

    fn signed(id: &str, bytes: &[u8]) -> ManifestAsset {
        ManifestAsset {
            id: id.to_string(),
            sha256: hex_lower(&sha256(bytes)),
            data_base64: BASE64_STANDARD.encode(bytes),
        }
    }

    fn webp_vp8l(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(b"VP8L");
        v.extend_from_slice(&[0, 0, 0, 0]); // chunk size (16..20)
        v.push(0x2f); // signature byte at offset 20
                      // 14-bit width-1 then 14-bit height-1, packed little-endian (21..25).
        let packed: u32 = (w - 1) | ((h - 1) << 14);
        v.extend_from_slice(&packed.to_le_bytes());
        v
    }

    fn webp_vp8(w: u16, h: u16) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(b"VP8 ");
        v.extend_from_slice(&[0; 10]); // chunk size + frame tag + start code (16..26)
        v.extend_from_slice(&w.to_le_bytes()); // 26..28
        v.extend_from_slice(&h.to_le_bytes()); // 28..30
        v
    }

    #[test]
    fn sniffs_png_gif_webp_dimensions() {
        assert_eq!(sniff(&png(100, 50)), Some((ImageFormat::Png, 100, 50)));
        assert_eq!(sniff(&gif(64, 48)), Some((ImageFormat::Gif, 64, 48)));
        assert_eq!(
            sniff(&webp_vp8x(300, 200)),
            Some((ImageFormat::Webp, 300, 200))
        );
        assert_eq!(
            sniff(&webp_vp8l(120, 90)),
            Some((ImageFormat::Webp, 120, 90))
        );
        assert_eq!(sniff(&webp_vp8(64, 32)), Some((ImageFormat::Webp, 64, 32)));
    }

    #[test]
    fn webp_vp8l_without_signature_byte_is_rejected() {
        let mut v = webp_vp8l(10, 10);
        v[20] = 0x00; // corrupt the 0x2f signature
        assert_eq!(sniff(&v), None);
    }

    #[test]
    fn image_format_extensions() {
        assert_eq!(ImageFormat::Png.ext(), "png");
        assert_eq!(ImageFormat::Gif.ext(), "gif");
        assert_eq!(ImageFormat::Webp.ext(), "webp");
    }

    fn ogg() -> Vec<u8> {
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 60]);
        v
    }

    fn wav() -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(&[0u8; 40]);
        v
    }

    #[test]
    fn sniffs_audio_containers() {
        assert_eq!(sniff_audio(&ogg()), Some(AudioFormat::Ogg));
        assert_eq!(sniff_audio(&wav()), Some(AudioFormat::Wav));
        assert_eq!(
            sniff_audio(&[0xff, 0xfb, 0x90, 0x00]),
            Some(AudioFormat::Mp3)
        );
        assert_eq!(sniff_audio(b"ID3\x04\x00"), Some(AudioFormat::Mp3));
        assert_eq!(sniff_audio(b"not audio"), None);
        // Too short to hold an MPEG frame sync.
        assert_eq!(sniff_audio(&[0xff]), None);
        // WebP's RIFF header must not be mistaken for WAV.
        assert_eq!(sniff_audio(&webp_vp8x(10, 10)), None);
    }

    #[test]
    fn validates_an_audio_asset_as_audio() {
        let (_, kind) = validate_asset(&signed("cue.ogg", &ogg())).unwrap();
        assert_eq!(kind, AssetKind::Audio(AudioFormat::Ogg));
        assert!(kind.is_audio());
        assert_eq!(kind.ext(), "ogg");
    }

    #[test]
    fn rejects_an_oversize_sound() {
        // Valid Ogg header padded past the sound cap (but under the image cap,
        // so it reaches the audio-specific check).
        let mut big = b"OggS".to_vec();
        big.resize(MAX_SOUND_BYTES + 1, 0);
        assert!(validate_asset(&signed("cue.ogg", &big))
            .unwrap_err()
            .contains("over the"));
    }

    #[test]
    fn audio_format_extensions() {
        assert_eq!(AudioFormat::Ogg.ext(), "ogg");
        assert_eq!(AudioFormat::Wav.ext(), "wav");
        assert_eq!(AudioFormat::Mp3.ext(), "mp3");
    }

    #[test]
    fn sniff_rejects_unknown_and_truncated() {
        assert_eq!(sniff(b"not an image"), None);
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
            None
        );
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPxxxx"), None);
    }

    #[test]
    fn validates_a_well_formed_asset() {
        let (bytes, kind) = validate_asset(&signed("twist.png", &png(100, 50))).unwrap();
        assert_eq!(kind, AssetKind::Image(ImageFormat::Png));
        assert_eq!(sniff(&bytes).unwrap().0, ImageFormat::Png);
    }

    #[test]
    fn rejects_unsafe_id() {
        let mut a = signed("../escape.png", &png(10, 10));
        a.sha256 = hex_lower(&sha256(&png(10, 10)));
        assert!(validate_asset(&a).unwrap_err().contains("must be 1..="));
        assert!(validate_asset(&signed("UPPER.png", &png(10, 10)))
            .unwrap_err()
            .contains("[a-z0-9._-]"));
    }

    #[test]
    fn rejects_bad_sha256_format() {
        let mut a = signed("a.png", &png(10, 10));
        a.sha256 = "tooshort".to_string();
        assert!(validate_asset(&a).unwrap_err().contains("64 lowercase hex"));
    }

    #[test]
    fn rejects_hash_mismatch() {
        let mut a = signed("a.png", &png(10, 10));
        a.sha256 = hex_lower(&sha256(b"different bytes"));
        assert!(validate_asset(&a).unwrap_err().contains("do not match"));
    }

    #[test]
    fn rejects_oversize_bytes() {
        let big = png(10, 10);
        let mut a = signed("a.png", &big);
        // Re-encode an over-cap blob and re-hash so only the size check trips.
        let payload = vec![0x89u8; MAX_ASSET_BYTES + 1];
        // Not a valid PNG, but the size check runs before sniffing.
        a.data_base64 = BASE64_STANDARD.encode(&payload);
        a.sha256 = hex_lower(&sha256(&payload));
        assert!(validate_asset(&a).unwrap_err().contains("over the"));
    }

    #[test]
    fn rejects_decompression_bomb_dimensions() {
        let bomb = png(40_000, 40_000); // 1.6e9 pixels
        assert!(validate_asset(&signed("a.png", &bomb))
            .unwrap_err()
            .contains("pixel cap"));
    }

    #[test]
    fn rejects_non_image_bytes() {
        let junk = b"this is plainly not an image at all".to_vec();
        assert!(validate_asset(&signed("a.png", &junk))
            .unwrap_err()
            .contains("not a supported image"));
    }

    #[test]
    fn rejects_an_empty_asset() {
        let a = ManifestAsset {
            id: "a.png".to_string(),
            sha256: hex_lower(&sha256(b"")),
            data_base64: String::new(),
        };
        assert!(validate_asset(&a).unwrap_err().contains("is empty"));
    }

    #[test]
    fn rejects_invalid_base64() {
        let a = ManifestAsset {
            id: "a.png".to_string(),
            sha256: hex_lower(&sha256(b"x")),
            data_base64: "not base64!!!".to_string(),
        };
        assert!(validate_asset(&a).unwrap_err().contains("not valid base64"));
    }
}
