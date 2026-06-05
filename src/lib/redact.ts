/**
 * Strip LemonSqueezy-shaped licence keys and `ENT1-…` manual tokens from a
 * string before it leaves the renderer (a log line, an error payload). A
 * stack trace or a backend response that captured a key would leak it
 * otherwise. The Rust side redacts again as defense-in-depth (see
 * `renderer_log.rs::redact_license_shapes`).
 */
export function redactRendererPayload(input: string): string {
  return input
    .replace(/ENT1-[A-Za-z0-9_-]{8,}/g, "[REDACTED-MANUAL-TOKEN]")
    .replace(/[A-Za-z0-9]{4}(?:-[A-Za-z0-9]{4}){3,}/g, "[REDACTED-LS-KEY]");
}

/**
 * Best-effort string form of an arbitrary value for logging. Strings pass
 * through; everything else is JSON-encoded, falling back to a placeholder
 * for `undefined` or values that can't be serialised (e.g. cyclic).
 */
export function stringifyForLog(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return "[unserialisable]";
  }
}
