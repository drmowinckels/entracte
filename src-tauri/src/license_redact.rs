//! Shared license-shape redaction for log lines that might capture a key.
//!
//! Both the renderer-error log path and the diagnostics-report tail run
//! through the same masker so any future change to key formats lands in
//! one place. The two key shapes we own:
//!
//! - Lemon Squeezy: `XXXX-XXXX-XXXX-XXXX...` — 4+ hyphen-separated
//!   4-char alphanumeric groups.
//! - Manual: `ENT1-…` — a base64url-no-pad tail of at least ~7 chars,
//!   so a filename like `ENT1-AB.log` doesn't get clobbered.
//!
//! Cheap regex-free scan; not a full secrets scanner.

pub fn redact_license_shapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 5 <= bytes.len() && &bytes[i..i + 5] == b"ENT1-" {
            let mut j = i + 5;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_')
            {
                j += 1;
            }
            if j - i >= 12 {
                out.push_str("[REDACTED-MANUAL-TOKEN]");
                i = j;
                continue;
            }
        }
        if looks_like_ls_key_at(bytes, i) {
            out.push_str("[REDACTED-LS-KEY]");
            i = ls_key_end(bytes, i);
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn looks_like_ls_key_at(bytes: &[u8], start: usize) -> bool {
    if start + 19 > bytes.len() {
        return false;
    }
    if !bytes[start..start + 4]
        .iter()
        .all(|b| b.is_ascii_alphanumeric())
    {
        return false;
    }
    let mut groups = 1;
    let mut i = start + 4;
    while groups < 4 && i + 5 <= bytes.len() && bytes[i] == b'-' {
        if !bytes[i + 1..i + 5]
            .iter()
            .all(|b| b.is_ascii_alphanumeric())
        {
            return false;
        }
        i += 5;
        groups += 1;
    }
    groups >= 4
}

fn ls_key_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 4;
    while i + 5 <= bytes.len() && bytes[i] == b'-' && {
        bytes[i + 1..i + 5]
            .iter()
            .all(|b| b.is_ascii_alphanumeric())
    } {
        i += 5;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_lemon_squeezy_key() {
        let got = redact_license_shapes("activated ABCD-1111-2222-3333 ok");
        assert!(got.contains("[REDACTED-LS-KEY]"), "got: {got}");
        assert!(!got.contains("ABCD-1111-2222-3333"));
    }

    #[test]
    fn masks_manual_token() {
        let got = redact_license_shapes("token=ENT1-AAAAAAAAAAAA_BBBB done");
        assert!(got.contains("[REDACTED-MANUAL-TOKEN]"), "got: {got}");
        assert!(!got.contains("ENT1-AAAAA"));
    }

    #[test]
    fn passes_innocent_text_through() {
        let got = redact_license_shapes("Error: undefined is not a function (foo.js:42)");
        assert_eq!(got, "Error: undefined is not a function (foo.js:42)");
    }

    #[test]
    fn keeps_short_ent1_prefix_intact() {
        // `ENT1-AB` is shorter than the 12-char minimum and must not be
        // redacted — otherwise legitimate text that happens to start with
        // `ENT1-` (e.g. a filename) gets clobbered.
        let got = redact_license_shapes("filename ENT1-AB.log");
        assert_eq!(got, "filename ENT1-AB.log");
    }

    #[test]
    fn rejects_ls_shape_with_non_alnum_group() {
        // Five hyphen-separated 4-char groups where one group contains a
        // non-alphanumeric char must NOT be redacted.
        let got = redact_license_shapes("ABCD-1111-22!2-3333-4444");
        assert!(!got.contains("[REDACTED-LS-KEY]"), "got: {got}");
    }

    #[test]
    fn extends_past_four_groups() {
        // A 5-group key gets consumed in one match (not "[REDACTED]-4444").
        let got = redact_license_shapes("key=ABCD-1111-2222-3333-4444 end");
        assert!(got.contains("[REDACTED-LS-KEY]"));
        assert!(!got.contains("4444"));
        assert!(got.contains("end"));
    }
}
