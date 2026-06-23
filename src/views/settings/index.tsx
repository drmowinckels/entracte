import { useState } from "react";
import { useCustomStylesheet } from "../../lib/use-custom-stylesheet";
import { useTauriListen } from "../../lib/use-tauri-listen";
import { OnboardingWizard } from "./components/onboarding/onboarding-wizard";
import { TABS } from "./constants";
import { useHooks } from "./hooks/use-hooks";
import { useOnboarding } from "./hooks/use-onboarding";
import { usePause } from "./hooks/use-pause";
import { useProfiles } from "./hooks/use-profiles";
import { useRovingTabList } from "./hooks/use-roving-tab-list";
import { useSettings } from "./hooks/use-settings";
import { useStats } from "./hooks/use-stats";
import { useSupporter } from "./hooks/use-supporter";
import { AboutTab } from "./tabs/about-tab";
import { BreaksTab } from "./tabs/breaks-tab";
import { InsightsTab } from "./tabs/insights-tab";
import { ProfilesTab } from "./tabs/profiles-tab";
import { QuietTab } from "./tabs/quiet-tab";
import { ScheduleTab } from "./tabs/schedule-tab";
import { SystemTab } from "./tabs/system-tab";
import type { Tab } from "./types";
import "./settings.css";

const TAB_IDS = TABS.map((t) => t.id);
const tabButtonId = (id: Tab) => `settings-tab-${id}`;
const tabPanelId = (id: Tab) => `settings-tabpanel-${id}`;

/** Top-level Settings window. Wires per-tab components together with
 * the cross-cutting hooks (`useSettings`, `useProfiles`, `useStats`,
 * `usePause`, `useHooks`). Shows a loading state until `settings` is
 * available, then renders the active tab. */
export default function Settings() {
  const [tab, setTab] = useState<Tab>("schedule");
  // Bumped when the backend's morning chore prompt fires; switches to the
  // Breaks tab and pulls focus to the chores input (see BreaksTab).
  const [chorePromptNonce, setChorePromptNonce] = useState(0);
  useTauriListen(
    "chores:prompt",
    () => {
      setTab("breaks");
      setChorePromptNonce((n) => n + 1);
    },
    [],
  );
  const { settings, update, reloadFromActive, setAutostart } = useSettings();
  const pauseInfo = usePause();
  const stats = useStats();
  const profiles = useProfiles();
  const hooks = useHooks(settings, reloadFromActive);
  const supporter = useSupporter();
  const onboarding = useOnboarding();
  useCustomStylesheet(settings?.custom_css ?? "");
  const { tablistProps, tabProps } = useRovingTabList<Tab>({
    ids: TAB_IDS,
    active: tab,
    onChange: setTab,
  });

  return (
    <>
      <a className="skip-link" href={`#${tabPanelId(tab)}`}>
        Skip to settings content
      </a>
      <main className="settings">
        {settings && onboarding.needed && (
          <OnboardingWizard
            settings={settings}
            update={update}
            setAutostart={setAutostart}
            onFinish={onboarding.complete}
          />
        )}
        <header className="settings-header">
          <div
            className="tabs"
            aria-label="Settings sections"
            {...tablistProps}
          >
            {TABS.map((t) => (
              <button
                key={t.id}
                id={tabButtonId(t.id)}
                aria-controls={tabPanelId(t.id)}
                className={tab === t.id ? "active" : ""}
                {...tabProps(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>
        </header>

        {!settings ? (
          <p className="loading">Loading…</p>
        ) : (
          <>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("schedule")}
              aria-labelledby={tabButtonId("schedule")}
              tabIndex={0}
              hidden={tab !== "schedule"}
            >
              <ScheduleTab settings={settings} update={update} />
            </div>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("breaks")}
              aria-labelledby={tabButtonId("breaks")}
              tabIndex={0}
              hidden={tab !== "breaks"}
            >
              <BreaksTab
                settings={settings}
                update={update}
                supporter={supporter.status}
                reload={reloadFromActive}
                focusChoresNonce={chorePromptNonce}
              />
            </div>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("quiet")}
              aria-labelledby={tabButtonId("quiet")}
              tabIndex={0}
              hidden={tab !== "quiet"}
            >
              <QuietTab
                settings={settings}
                update={update}
                pauseInfo={pauseInfo}
              />
            </div>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("system")}
              aria-labelledby={tabButtonId("system")}
              tabIndex={0}
              hidden={tab !== "system"}
            >
              <SystemTab
                settings={settings}
                update={update}
                setAutostart={setAutostart}
                hooks={hooks}
                reload={reloadFromActive}
              />
            </div>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("insights")}
              aria-labelledby={tabButtonId("insights")}
              tabIndex={0}
              hidden={tab !== "insights"}
            >
              <InsightsTab stats={stats} />
            </div>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("profiles")}
              aria-labelledby={tabButtonId("profiles")}
              tabIndex={0}
              hidden={tab !== "profiles"}
            >
              <ProfilesTab profiles={profiles} />
            </div>
            <div
              className="tab-content"
              role="tabpanel"
              id={tabPanelId("about")}
              aria-labelledby={tabButtonId("about")}
              tabIndex={0}
              hidden={tab !== "about"}
            >
              <AboutTab
                supporter={supporter}
                settings={settings}
                updateSetting={update}
              />
            </div>
          </>
        )}
      </main>
    </>
  );
}
