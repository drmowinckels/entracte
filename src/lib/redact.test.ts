import { describe, expect, it } from "vitest";
import { redactRendererPayload, stringifyForLog } from "./redact";

describe("redactRendererPayload", () => {
  it("redacts ENT1- manual tokens", () => {
    expect(redactRendererPayload("got ENT1-abcdef0123456789 here")).toContain(
      "[REDACTED-MANUAL-TOKEN]",
    );
  });

  it("redacts LemonSqueezy-shaped keys", () => {
    expect(redactRendererPayload("key ABCD-1111-2222-3333 ok")).toContain(
      "[REDACTED-LS-KEY]",
    );
  });

  it("leaves ordinary text untouched", () => {
    expect(redactRendererPayload("nothing secret here")).toBe(
      "nothing secret here",
    );
  });
});

describe("stringifyForLog", () => {
  it("passes strings through unchanged", () => {
    expect(stringifyForLog("hello")).toBe("hello");
  });

  it("JSON-encodes objects", () => {
    expect(stringifyForLog({ a: 1, b: "x" })).toBe('{"a":1,"b":"x"}');
  });

  it("falls back for undefined", () => {
    expect(stringifyForLog(undefined)).toBe("undefined");
  });

  it("falls back for unserialisable (cyclic) values", () => {
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(stringifyForLog(cyclic)).toBe("[unserialisable]");
  });
});
