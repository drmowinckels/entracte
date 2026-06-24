import { describe, expect, it } from "vitest";
import { dateFieldOrder, monthNames } from "./locale-format";

describe("dateFieldOrder", () => {
  it("is month-first for en-US", () => {
    expect(dateFieldOrder("en-US")).toEqual(["month", "day", "year"]);
  });

  it("is day-first for en-GB", () => {
    expect(dateFieldOrder("en-GB")).toEqual(["day", "month", "year"]);
  });

  it("is year-first for ja-JP", () => {
    expect(dateFieldOrder("ja-JP")).toEqual(["year", "month", "day"]);
  });

  it("falls back to ISO order for an unusable locale", () => {
    expect(dateFieldOrder("")).toEqual(["year", "month", "day"]);
  });
});

describe("monthNames", () => {
  it("returns twelve names, January first", () => {
    const names = monthNames("en-US");
    expect(names).toHaveLength(12);
    expect(names[0]).toBe("January");
    expect(names[11]).toBe("December");
  });

  it("localises to the requested locale", () => {
    // A non-English locale yields different month names than en-US, proving
    // the locale argument is honoured (exact words vary by ICU data).
    expect(monthNames("es-ES")[0]).not.toBe(monthNames("en-US")[0]);
  });
});
