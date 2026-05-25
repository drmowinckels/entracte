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
    let mut redacted = crate::license_redact::redact_license_shapes(input);
    if redacted.len() > MAX_FIELD_BYTES {
        let mut cut = MAX_FIELD_BYTES;
        while cut > 0 && !redacted.is_char_boundary(cut) {
            cut -= 1;
        }
        redacted.truncate(cut);
        redacted.push_str("…[truncated]");
    }
    redacted
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
    fn redact_and_truncate_caps_oversized_input() {
        let huge = "x".repeat(MAX_FIELD_BYTES * 2);
        let got = redact_and_truncate(&huge);
        assert!(got.len() < MAX_FIELD_BYTES * 2);
        assert!(got.ends_with("…[truncated]"));
    }

    #[test]
    fn redact_and_truncate_masks_keys_before_truncating() {
        // Sanity check that the truncation path still routes through the
        // shared masker — a key in the first 8 KiB must come out redacted.
        let got = redact_and_truncate("oops ABCD-1111-2222-3333 in scope");
        assert!(got.contains("[REDACTED-LS-KEY]"));
        assert!(!got.contains("ABCD-1111-2222-3333"));
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
