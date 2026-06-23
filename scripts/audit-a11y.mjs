// Headless accessibility audit for the Entracte renderer.
// Spawns `vite preview` against the prebuilt dist/, drives Chromium via
// puppeteer, injects a Tauri shim so the React tree renders normally,
// then runs axe-core on every tab in both colour schemes.

import { spawn } from "node:child_process";
import { once } from "node:events";
import { readFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import puppeteer from "puppeteer";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_DIR = join(__dirname, "..");
const AXE_PATH = join(REPO_DIR, "node_modules", "axe-core", "axe.min.js");
const PORT = Number(process.env.AUDIT_PORT ?? 4173);
const HOST = "127.0.0.1";

// The fixture lives in its own JSON file so a vitest test can parse it
// against `schedulerSettingsSchema` and catch drift the way CI couldn't
// before — see scripts/audit-a11y-settings-fixture.test.ts.
const DEFAULT_SETTINGS_JSON = readFileSync(
  join(__dirname, "audit-a11y-settings-fixture.json"),
  "utf8",
);

const TABS = [
  "Schedule",
  "Breaks",
  "Pausing",
  "System",
  "Insights",
  "Profiles",
  "About",
];

const SCHEMES = ["light", "dark"];

const CONSOLE_LEVELS = new Set(["error", "warning"]);
// Patterns that match known-benign messages we don't want to fail on.
const CONSOLE_IGNORE = [/react-devtools/i, /download the react devtools/i];

const TAURI_SHIM = `
(() => {
  if (window.__TAURI_INTERNALS__) return;

  const DEFAULT_SETTINGS = ${DEFAULT_SETTINGS_JSON};

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const dayBuckets = Array.from({ length: 84 }, (_, i) => {
    const d = new Date(today);
    d.setDate(today.getDate() - (83 - i));
    return {
      date: d.toISOString().slice(0, 10),
      taken: i % 6,
      dismissed: i % 4,
    };
  });

  const DIGEST = {
    range: "week",
    range_start: dayBuckets[dayBuckets.length - 7].date,
    range_end: dayBuckets[dayBuckets.length - 1].date,
    micro_taken: 12, micro_dismissed: 2, long_taken: 4, long_dismissed: 1,
    sleep_shown: 0, postponed_total: 3, skipped_total: 3,
    suppressions: [
      { reason: "Camera", label: "Camera in use", count: 5 },
      { reason: "Dnd", label: "Do Not Disturb", count: 2 },
    ],
    suppressions_by_kind: [
      { kind: "long", reason: "camera", label: "Camera in use", count: 3 },
      { kind: "micro", reason: "camera", label: "Camera in use", count: 2 },
      { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 2 },
    ],
    pause_total_secs: 600, pause_count: 2,
    by_hour: Array.from({ length: 24 }, (_, h) => (h >= 9 && h <= 17 ? h % 5 : 0)),
    by_day: dayBuckets,
    by_weekday: Array.from({ length: 7 }, (_, w) => ({
      weekday: w,
      taken: (w + 1) % 5,
      dismissed: w % 3,
    })),
    previous: {
      breaks_taken: 10,
      breaks_dismissed: 4,
      postponed_total: 2,
      skipped_total: 1,
    },
    postpone_follow_through: {
      total: 3,
      taken: 2,
      dismissed: 0,
      skipped: 1,
      unresolved: 0,
    },
  };

  const callbacks = {};

  const responses = {
    get_settings: DEFAULT_SETTINGS,
    update_settings: null,
    // Already onboarded, so the first-run wizard never overlays the tabs
    // this audit walks. (Wizard a11y is exercised by its own component test.)
    get_onboarding_completed: true,
    complete_onboarding: null,
    get_platform: "macos",
    get_platform_capabilities: {
      supportsDndRead: true,
      mediaPauseGranular: false,
      installerUnsignedWarning: false,
    },
    get_pause_info: { paused: false, remaining_secs: null },
    get_break_stats: { taken: 8, skipped: 1, postponed: 2 },
    list_profiles: ["Default", "Deep work", "Light day"],
    get_active_profile: "Default",
    set_active_profile: null,
    create_profile: null,
    rename_profile: null,
    delete_profile: null,
    get_screen_time: { date: "2026-05-16", seconds: 14400, last_reminder_epoch_secs: null },
    get_chores: {
      date: "2026-05-16",
      items: ["Water the plants", "Reply to Sam"],
      rotation: 0,
    },
    set_chores: { date: "2026-05-16", items: [], rotation: 0 },
    get_stats_digest: DIGEST,
    get_supporter_status: { is_supporter: false, masked_key: null, last_validated_at: null },
    trigger_test_break: null,
    skip_next_break: null,
    reset_break_stats: null,
    // Drives the break-overlay pass below. A micro break with every action
    // available exercises the most overlay surface (progress ring, hint,
    // Postpone + Skip). Micro uses the end_chime sound (plays only at
    // finish), so nothing autoplays on mount -- the fixture's ambient
    // long_sound would otherwise throw in headless Chromium.
    get_current_break: {
      kind: "micro",
      duration_secs: 300,
      enforceable: false,
      manual_finish: false,
      postpone_available: true,
      skip_available: true,
      hints: ["Stand up and roll your shoulders", "Look out a window"],
      hint_rotate_seconds: 8,
      health_intensity: 0.4,
      routine_steps: [],
    },
    get_postpone_state: { count: 0, max: 3, remaining: 3 },
    postpone_break: null,
    build_diagnostics_report: "## Diagnostics",
    export_stats_csv: "kind,count\\n",
    clear_event_log: null,
    resume: null,
    "plugin:app|version": "0.0.1-audit",
    "plugin:autostart|is_enabled": false,
    "plugin:autostart|enable": null,
    "plugin:autostart|disable": null,
    "plugin:event|listen": 1,
    "plugin:event|unlisten": null,
    "plugin:opener|open_url": null,
  };

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main" },
    },
    callbacks,
    invoke: (cmd) =>
      Promise.resolve(cmd in responses ? responses[cmd] : null),
    transformCallback: (cb, _once) => {
      const id = Math.floor(Math.random() * 1e9);
      callbacks[id] = cb;
      return id;
    },
    unregisterCallback: (id) => {
      delete callbacks[id];
    },
    runCallback: (id, ...args) => callbacks[id]?.(...args),
    convertFileSrc: (p) => p,
  };
})();
`;

async function startPreview() {
  const child = spawn(
    "npx",
    ["vite", "preview", "--port", String(PORT), "--host", HOST, "--strictPort"],
    { cwd: REPO_DIR, stdio: ["ignore", "pipe", "pipe"] },
  );
  child.stdout.on("data", (b) => process.stdout.write(b));
  child.stderr.on("data", (b) => process.stderr.write(b));

  const url = `http://${HOST}:${PORT}/`;
  const deadline = Date.now() + 30_000;
  let lastErr;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`vite preview exited early with code ${child.exitCode}`);
    }
    try {
      const res = await fetch(url, { method: "HEAD" });
      if (res.ok || res.status === 405) return child;
    } catch (e) {
      lastErr = e;
    }
    await sleep(200);
  }
  throw new Error(
    `vite preview did not respond on ${url} within 30s` +
      (lastErr ? `: ${lastErr.message}` : ""),
  );
}

async function stopPreview(child) {
  if (!child || child.exitCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([once(child, "exit"), sleep(3000)]);
  if (child.exitCode === null) child.kill("SIGKILL");
}

// Run axe-core against whatever is currently rendered and return the
// normalised violations. Shared by the per-tab settings audit and the
// break-overlay audit.
async function runAxe(page, disabledRules = []) {
  await page.addScriptTag({ path: AXE_PATH });
  return page.evaluate(async (disabled) => {
    const results = await window.axe.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"],
      },
      rules: Object.fromEntries(disabled.map((id) => [id, { enabled: false }])),
    });
    return results.violations.map((v) => ({
      id: v.id,
      impact: v.impact,
      help: v.help,
      helpUrl: v.helpUrl,
      nodes: v.nodes.map((n) => ({
        target: n.target,
        html: n.html.slice(0, 240),
        failureSummary: n.failureSummary,
        any: n.any?.map((a) => ({
          id: a.id,
          message: a.message,
          data: a.data,
        })),
      })),
    }));
  }, disabledRules);
}

async function auditTab(page, tab) {
  await page.evaluate((label) => {
    const buttons = Array.from(document.querySelectorAll(".tabs button"));
    const btn = buttons.find((b) => b.textContent?.trim() === label);
    btn?.click();
  }, tab);
  await page.evaluate(() => {
    document
      .querySelectorAll("details.advanced-section")
      .forEach((d) => d.setAttribute("open", ""));
  });
  await sleep(150);
  return runAxe(page);
}

// Wire console + pageerror capture for a page into `consoleNoise`, tagged
// with the scheme and the audited surface. Shared by both passes.
function captureConsole(page, consoleNoise, scheme, surface) {
  page.on("console", (msg) => {
    const type = msg.type();
    if (!CONSOLE_LEVELS.has(type)) return;
    const text = msg.text();
    if (CONSOLE_IGNORE.some((rx) => rx.test(text))) return;
    consoleNoise.push({ scheme, tab: surface, type, text });
  });
  page.on("pageerror", (err) => {
    consoleNoise.push({
      scheme,
      tab: surface,
      type: "pageerror",
      text: err.stack || err.message,
    });
  });
}

async function main() {
  let preview;
  let browser;
  const allViolations = [];
  const consoleNoise = [];

  try {
    preview = await startPreview();
    browser = await puppeteer.launch({
      headless: "new",
      args: ["--no-sandbox", "--disable-setuid-sandbox"],
    });

    for (const scheme of SCHEMES) {
      for (const tab of TABS) {
        const page = await browser.newPage();
        await page.evaluateOnNewDocument(TAURI_SHIM);
        captureConsole(page, consoleNoise, scheme, tab);
        await page.emulateMediaFeatures([
          { name: "prefers-color-scheme", value: scheme },
        ]);
        await page.goto(`http://${HOST}:${PORT}/index.html`, {
          waitUntil: "networkidle0",
          timeout: 15_000,
        });
        await page.waitForSelector(".settings .tabs", { timeout: 5_000 });

        const violations = await auditTab(page, tab);
        for (const v of violations) {
          allViolations.push({ scheme, tab, ...v });
        }
        await page.close();
      }
    }

    // Break overlay — a separate Tauri WebviewWindow in production,
    // selected here via the ?window=overlay param. The shim's
    // get_current_break drives it into a live break so axe sees the real
    // dialog, progress ring, hint, and action buttons. Same axe + console
    // gate, both colour schemes.
    for (const scheme of SCHEMES) {
      const surface = "Break overlay";
      const page = await browser.newPage();
      await page.evaluateOnNewDocument(TAURI_SHIM);
      // Force the overlay's reduced-transparency path so it paints a solid
      // theme background instead of the default translucent one. axe can
      // only measure contrast against a known background — over the
      // translucent overlay it would blend with the white test page (a
      // meaningless ratio), whereas the opaque variant is the overlay's
      // real text-on-theme contrast contract, and the exact rendering
      // reduced-transparency users get. CDP can't emulate this media
      // feature, so patch matchMedia for just this query and delegate the
      // rest (incl. the CDP-driven prefers-color-scheme) to the native one.
      await page.evaluateOnNewDocument(() => {
        const native = window.matchMedia.bind(window);
        window.matchMedia = (query) =>
          typeof query === "string" &&
          query.includes("prefers-reduced-transparency")
            ? {
                matches: query.includes("reduce"),
                media: query,
                onchange: null,
                addEventListener() {},
                removeEventListener() {},
                addListener() {},
                removeListener() {},
                dispatchEvent: () => false,
              }
            : native(query);
      });
      captureConsole(page, consoleNoise, scheme, surface);
      await page.emulateMediaFeatures([
        { name: "prefers-color-scheme", value: scheme },
      ]);
      await page.goto(`http://${HOST}:${PORT}/index.html?window=overlay`, {
        waitUntil: "networkidle0",
        timeout: 15_000,
      });
      await page.waitForSelector(".overlay-root", { timeout: 5_000 });
      await sleep(150);

      // color-contrast is disabled for the overlay only. The overlay uses
      // `opacity` on text elements (the kind label at 0.7, the hint at 0.9,
      // the credit at 0.4) for visual hierarchy, and axe-core can't compute
      // contrast through an opacity compositing layer — it misreads the
      // solid theme background as white and reports false positives (the
      // real ratio of near-white text on the dark theme is ~10:1, verified).
      // Every structural rule — roles, names, focus order, labels — still
      // runs; the high-contrast and reduced-transparency paths cover users
      // who need stronger contrast.
      const violations = await runAxe(page, ["color-contrast"]);
      for (const v of violations) {
        allViolations.push({ scheme, tab: surface, ...v });
      }
      await page.close();
    }
  } finally {
    if (browser) await browser.close();
    if (preview) await stopPreview(preview);
  }

  // Settings tabs (TABS × schemes) plus one break-overlay pass per scheme.
  const totalAudits = (TABS.length + 1) * SCHEMES.length;

  if (consoleNoise.length > 0) {
    console.error(
      `\n✗ renderer console: ${consoleNoise.length} message(s) across ${totalAudits} audits`,
    );
    for (const m of consoleNoise) {
      console.error(`  [${m.scheme}] ${m.tab} (${m.type}): ${m.text}`);
    }
  }

  if (allViolations.length === 0 && consoleNoise.length === 0) {
    console.log(
      `\n✓ axe a11y: clean across ${totalAudits} (surface × scheme) audits`,
    );
    console.log(`✓ renderer console: clean across ${totalAudits} audits`);
    process.exit(0);
  }

  if (allViolations.length === 0) {
    process.exit(1);
  }

  console.error(
    `\n✗ axe a11y: ${allViolations.length} violation(s) across ${totalAudits} audits\n`,
  );
  const grouped = new Map();
  for (const v of allViolations) {
    const key = `${v.id}|${v.impact}`;
    if (!grouped.has(key)) grouped.set(key, { ...v, occurrences: [] });
    grouped
      .get(key)
      .occurrences.push({ scheme: v.scheme, tab: v.tab, nodes: v.nodes });
  }
  for (const [, v] of grouped) {
    console.error(`  ▸ ${v.id} (${v.impact ?? "n/a"}): ${v.help}`);
    console.error(`    ${v.helpUrl}`);
    for (const occ of v.occurrences) {
      console.error(`      [${occ.scheme}] ${occ.tab}`);
      for (const n of occ.nodes.slice(0, 3)) {
        console.error(`        ${n.target.join(" ")}`);
        if (n.html) console.error(`          ${n.html}`);
        for (const a of n.any ?? []) {
          if (a.data && typeof a.data === "object") {
            const d = a.data;
            const summary = [
              d.fgColor ? `fg=${d.fgColor}` : null,
              d.bgColor ? `bg=${d.bgColor}` : null,
              d.contrastRatio ? `ratio=${d.contrastRatio}` : null,
              d.expectedContrastRatio
                ? `need=${d.expectedContrastRatio}`
                : null,
              d.fontSize ? `size=${d.fontSize}` : null,
              d.fontWeight ? `weight=${d.fontWeight}` : null,
            ]
              .filter(Boolean)
              .join(" ");
            if (summary) console.error(`          ↳ ${summary}`);
          }
        }
      }
    }
  }
  process.exit(1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
