import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import type { ZodIssue, ZodType } from "zod";
import { redactRendererPayload, stringifyForLog } from "./redact";

export class IpcError extends Error {
  readonly command: string;
  readonly issues: ZodIssue[];
  readonly received: unknown;

  constructor(command: string, issues: ZodIssue[], received: unknown) {
    super(
      `IPC response for "${command}" failed schema validation: ${issues
        .map((i) => `${i.path.join(".") || "<root>"} – ${i.message}`)
        .join("; ")}`,
    );
    this.name = "IpcError";
    this.command = command;
    this.issues = issues;
    this.received = received;
  }
}

export async function invoke<T>(
  cmd: string,
  args: InvokeArgs | undefined,
  schema: ZodType<T>,
): Promise<T> {
  const raw = await tauriInvoke(cmd, args);
  const parsed = schema.safeParse(raw);
  if (!parsed.success) {
    const err = new IpcError(cmd, parsed.error.issues, raw);
    // Redact the raw payload before it hits the devtools console — a
    // malformed backend response could carry a licence key, and this is the
    // one place the renderer logs a backend value verbatim. `received` is
    // kept raw on the error itself (never transmitted; callers inspect it
    // in-process).
    console.error(err.message, {
      issues: err.issues,
      received: redactRendererPayload(stringifyForLog(raw)),
    });
    throw err;
  }
  return parsed.data;
}
