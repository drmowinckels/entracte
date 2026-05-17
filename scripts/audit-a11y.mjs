// Headless accessibility audit for the Entracte renderer.
// Spawns `vite preview` against the prebuilt dist/, drives Chromium via
// puppeteer, injects a Tauri shim so the React tree renders normally,
// then runs axe-core on every tab in both colour schemes.

import { spawn } from "node:child_process";
import { once } from "node:events";
import { setTimeout as sleep } from "node:timers/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import puppeteer from "puppeteer";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_DIR = join(__dirname, "..");
const AXE_PATH = join(REPO_DIR, "node_modules", "axe-core", "axe.min.js");
const PORT = Number(process.env.AUDIT_PORT ?? 4173);
const HOST = "127.0.0.1";

const TABS = [
  "Schedule",
  "Breaks",
  "Quiet times",
  "System",
  "Insights",
  "Profiles",
  "About",
];

const SCHEMES = ["light", "dark"];

const CONSOLE_LEVELS = new Set(["error", "warning"]);
// Patterns that match known-benign messages we don't want to fail on.
const CONSOLE_IGNORE = [
  /react-devtools/i,
  /download the react devtools/i,
];

const TAURI_SHIM = `
(() => {
  if (window.__TAURI_INTERNALS__) return;

  const DEFAULT_SETTINGS = {
    micro_interval_secs: 1200, micro_duration_secs: 20,
    long_interval_secs: 3000, long_duration_secs: 600,
    micro_idle_reset_secs: 300, long_idle_reset_secs: 300,
    micro_enabled: true, long_enabled: true,
    micro_enforceable: false, long_enforceable: true,
    pause_during_dnd: true, pause_during_camera: true, pause_during_video: false,
    work_window_enabled: true, work_start_minutes: 540, work_end_minutes: 1020,
    bedtime_enabled: true, bedtime_start_minutes: 1320, bedtime_end_minutes: 1380,
    bedtime_interval_secs: 300, bedtime_duration_secs: 30,
    prebreak_notification_enabled: true, prebreak_notification_seconds: 30,
    overlay_opacity: 0.5, overlay_color: "custom", overlay_custom_rgb: "31, 41, 58",
    overlay_high_contrast: false,
    show_hint: true, monitor_placement: "primary",
    strict_mode: false, postpone_enabled: true, postpone_minutes: 5,
    postpone_escalation_enabled: true, postpone_escalation_step_secs: 60, postpone_max_count: 3,
    show_current_time: true, clock_format: "24h",
    micro_manual_finish: false, long_manual_finish: false,
    autostart_enabled: false,
    micro_sound: { mode: "end_chime", sound_id: "337048" },
    long_sound: { mode: "ambient", sound_id: "851196" },
    sound_volume: 0.5,
    app_pause_enabled: true, app_pause_list: ["zoom"],
    break_health_enabled: true,
    micro_physical_hints: ["Look 20 feet away."],
    micro_psychological_hints: ["Unclench your jaw."],
    micro_hint_mix: "both",
    long_hints: ["Stand up. Stretch."],
    long_social_hints: ["Call someone you love.", "Step outside with a colleague."],
    long_hint_mix: "both",
    sleep_hints: ["Time to wind down."],
    hint_rotate_seconds: 12,
    delay_break_if_typing: true, typing_grace_secs: 10, typing_max_deferral_secs: 60,
    pause_countdown_if_typing: false,
    overlay_font_scale: 1.0,
    micro_fixed_times: [], long_fixed_times: [],
    micro_schedule_mode: "both", long_schedule_mode: "both",
    hooks_enabled: true, hooks: [{ event: "break_start", command: "echo hi", enabled: true }],
    daily_screen_time_enabled: true,
    daily_screen_time_budget_minutes: 480,
    daily_screen_time_remind_again_minutes: 30,
    tray_countdown_enabled: true, tray_countdown_target: "next",
    micro_break_mode: "overlay", long_break_mode: "overlay",
  };

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
    pause_total_secs: 600, pause_count: 2,
    by_hour: Array.from({ length: 24 }, (_, h) => (h >= 9 && h <= 17 ? h % 5 : 0)),
    by_day: dayBuckets,
  };

  const callbacks = {};

  const responses = {
    get_settings: DEFAULT_SETTINGS,
    update_settings: null,
    get_platform: "macos",
    get_pause_info: { paused: false, remaining_secs: null },
    get_break_stats: { taken: 8, skipped: 1, postponed: 2 },
    list_profiles: ["Default", "Deep work", "Light day"],
    get_active_profile: "Default",
    set_active_profile: null,
    create_profile: null,
    rename_profile: null,
    delete_profile: null,
    get_screen_time: { date: "2026-05-16", seconds: 14400, last_reminder_epoch_secs: null },
    get_stats_digest: DIGEST,
    get_supporter_status: { is_supporter: false, masked_key: null, last_validated_at: null },
    trigger_test_break: null,
    skip_next_break: null,
    reset_break_stats: null,
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

  await page.addScriptTag({ path: AXE_PATH });
  const violations = await page.evaluate(async () => {
    const results = await window.axe.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"],
      },
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
        any: n.any?.map((a) => ({ id: a.id, message: a.message, data: a.data })),
      })),
    }));
  });
  return violations;
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
        page.on("console", (msg) => {
          const type = msg.type();
          if (!CONSOLE_LEVELS.has(type)) return;
          const text = msg.text();
          if (CONSOLE_IGNORE.some((rx) => rx.test(text))) return;
          consoleNoise.push({ scheme, tab, type, text });
        });
        page.on("pageerror", (err) => {
          consoleNoise.push({
            scheme,
            tab,
            type: "pageerror",
            text: err.stack || err.message,
          });
        });
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
  } finally {
    if (browser) await browser.close();
    if (preview) await stopPreview(preview);
  }

  const totalTabs = TABS.length * SCHEMES.length;

  if (consoleNoise.length > 0) {
    console.error(`\n✗ renderer console: ${consoleNoise.length} message(s) across ${totalTabs} audits`);
    for (const m of consoleNoise) {
      console.error(`  [${m.scheme}] ${m.tab} (${m.type}): ${m.text}`);
    }
  }

  if (allViolations.length === 0 && consoleNoise.length === 0) {
    console.log(`\n✓ axe a11y: clean across ${totalTabs} (tab × scheme) audits`);
    console.log(`✓ renderer console: clean across ${totalTabs} audits`);
    process.exit(0);
  }

  if (allViolations.length === 0) {
    process.exit(1);
  }

  console.error(`\n✗ axe a11y: ${allViolations.length} violation(s) across ${totalTabs} audits\n`);
  const grouped = new Map();
  for (const v of allViolations) {
    const key = `${v.id}|${v.impact}`;
    if (!grouped.has(key)) grouped.set(key, { ...v, occurrences: [] });
    grouped.get(key).occurrences.push({ scheme: v.scheme, tab: v.tab, nodes: v.nodes });
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
              d.expectedContrastRatio ? `need=${d.expectedContrastRatio}` : null,
              d.fontSize ? `size=${d.fontSize}` : null,
              d.fontWeight ? `weight=${d.fontWeight}` : null,
            ].filter(Boolean).join(" ");
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
