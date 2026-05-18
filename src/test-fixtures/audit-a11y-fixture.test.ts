import { describe, expect, it } from "vitest";
import fixture from "../../scripts/audit-a11y-settings-fixture.json";
import { schedulerSettingsSchema } from "../views/settings/hooks/use-settings";

// The headless a11y audit (`scripts/audit-a11y.mjs`) mocks the
// `get_settings` IPC with this fixture. If a new field is added to
// `SchedulerSettings` and the fixture isn't updated, the renderer's Zod
// validation rejects the response and every tab errors out. CI used to
// be the first place that surfaced — this test catches it at unit-test
// time.
describe("audit-a11y fixture", () => {
  it("parses against the scheduler settings schema", () => {
    expect(() => schedulerSettingsSchema.parse(fixture)).not.toThrow();
  });
});
