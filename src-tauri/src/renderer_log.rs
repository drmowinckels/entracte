//! Renderer-side error reporter. The web error boundary calls this
//! from `componentDidCatch` so the crash lands in the same rotating
//! log file as everything else, instead of vanishing into the webview
//! devtools console where no end user ever looks.

/// Per-field cap. The renderer can be compromised (supporter custom
/// CSS is a real injection surface), so we treat the message/stack as
/// attacker-controlled and refuse to log more than ~8 KiB per field.
/// Real stack traces are well under this; a 4 GiB stack on a tight
/// loop is a disk-fill DoS.
const MAX_FIELD_BYTES: usize = 8 * 1024;

#[tauri::command]
pub fn report_renderer_error(
    message: String,
    stack: Option<String>,
    component_stack: Option<String>,
) {
    let message = redact_and_truncate(&message);
    let stack = stack
        .as_deref()
        .map(redact_and_truncate)
        .unwrap_or_else(|| "<none>".to_string());
    let component_stack = component_stack
        .as_deref()
        .map(redact_and_truncate)
        .unwrap_or_else(|| "<none>".to_string());
    log::error!("renderer: {message} | stack={stack} | component_stack={component_stack}");
}

/// Bound the field at `MAX_FIELD_BYTES` *and* strip anything that
/// looks like a Lemon Squeezy licence key or a manual `ENT1-…` token
/// — the renderer should never have these in scope, but a stack trace
/// that captured local variables (or a user CSS that embedded one)
/// would otherwise leak the secret into the log file.
fn redact_and_truncate(input: &str) -> String {
    let mut redacted = redact_license_shapes(input);
    if redacted.len() > MAX_FIELD_BYTES {
        // Truncate at a char boundary so we don't slice a UTF-8 codepoint.
        let mut cut = MAX_FIELD_BYTES;
        while cut > 0 && !redacted.is_char_boundary(cut) {
            cut -= 1;
        }
        redacted.truncate(cut);
        redacted.push_str("…[truncated]");
    }
    redacted
}

/// Replace LemonSqueezy-shaped keys (`XXXX-XXXX-XXXX-XXXX...`, 4+
/// hyphen-separated 4-char groups) and manual tokens (`ENT1-…`) with
/// `[REDACTED-LS-KEY]` / `[REDACTED-MANUAL-TOKEN]`. Cheap regex-free
/// scan; not a full secrets scanner, but covers the two shapes we own.
fn redact_license_shapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Manual token: ENT1-...
        if i + 5 <= bytes.len() && &bytes[i..i + 5] == b"ENT1-" {
            // Consume until the next character that can't appear in the
            // base64url tail (we use base64-no-pad).
            let mut j = i + 5;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_')
            {
                j += 1;
            }
            // Must have consumed at least a few chars to count as a token.
            if j - i >= 12 {
                out.push_str("[REDACTED-MANUAL-TOKEN]");
                i = j;
                continue;
            }
        }
        // LemonSqueezy-shaped key: 4 groups of 4 alphanumeric, separated by '-'.
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
    // Pattern: 4 alphanum, then 3+ repetitions of '-' + 4 alphanum.
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
    // Skip the matched 4 alphanum groups (and any further dash-separated alphanum).
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
    fn report_renderer_error_accepts_full_args() {
        report_renderer_error(
            "boom".to_string(),
            Some("at foo (a.js:1)".to_string()),
            Some("in <App/>".to_string()),
        );
    }

    #[test]
    fn report_renderer_error_accepts_missing_optionals() {
        report_renderer_error("boom".to_string(), None, None);
    }

    #[test]
    fn redact_license_shapes_masks_lemon_squeezy_key() {
        let got = redact_license_shapes("activated ABCD-1111-2222-3333 ok");
        assert!(got.contains("[REDACTED-LS-KEY]"), "got: {got}");
        assert!(!got.contains("ABCD-1111-2222-3333"));
    }

    #[test]
    fn redact_license_shapes_masks_manual_token() {
        let got = redact_license_shapes("token=ENT1-AAAAAAAAAAAA_BBBB done");
        assert!(got.contains("[REDACTED-MANUAL-TOKEN]"), "got: {got}");
        assert!(!got.contains("ENT1-AAAAA"));
    }

    #[test]
    fn redact_license_shapes_passes_through_innocent_text() {
        let got = redact_license_shapes("Error: undefined is not a function (foo.js:42)");
        assert_eq!(got, "Error: undefined is not a function (foo.js:42)");
    }

    #[test]
    fn redact_and_truncate_caps_oversized_input() {
        let huge = "x".repeat(MAX_FIELD_BYTES * 2);
        let got = redact_and_truncate(&huge);
        assert!(got.len() < MAX_FIELD_BYTES * 2);
        assert!(got.ends_with("…[truncated]"));
    }

    #[test]
    fn redact_and_truncate_respects_char_boundary() {
        // Use a multi-byte UTF-8 codepoint (3 bytes) right at the cap edge.
        let mut s = "a".repeat(MAX_FIELD_BYTES - 1);
        s.push('日'); // 3 bytes — pushes us 2 bytes past the cap
        s.push_str("more");
        let got = redact_and_truncate(&s);
        assert!(got.ends_with("…[truncated]"));
        // Should not have sliced mid-codepoint (no panic, valid UTF-8).
        let _ = std::str::from_utf8(got.as_bytes()).expect("must be valid utf-8");
    }
}
