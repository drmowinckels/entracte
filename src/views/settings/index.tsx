import { useState } from "react";
import { useCustomStylesheet } from "../../lib/use-custom-stylesheet";
import { TABS } from "./constants";
import { useHooks } from "./hooks/use-hooks";
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
  const { settings, update, updateMany, reloadFromActive, setAutostart } =
    useSettings();
  const pauseInfo = usePause();
  const stats = useStats();
  const profiles = useProfiles();
  const hooks = useHooks(settings, reloadFromActive);
  const supporter = useSupporter();
  useCustomStylesheet(settings?.custom_css ?? "");
  const { tablistProps, tabProps } = useRovingTabList<Tab>({
    ids: TAB_IDS,
    active: tab,
    onChange: setTab,
  });

  return (
    <main className="settings">
      <header className="settings-header">
        <nav
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
        </nav>
      </header>

      {!settings ? (
        <p className="loading">Loading…</p>
      ) : (
        <div
          className="tab-content"
          role="tabpanel"
          id={tabPanelId(tab)}
          aria-labelledby={tabButtonId(tab)}
          tabIndex={0}
        >
          {tab === "schedule" && (
            <ScheduleTab
              settings={settings}
              update={update}
              updateMany={updateMany}
              supporter={supporter.status}
            />
          )}
          {tab === "breaks" && (
            <BreaksTab
              settings={settings}
              update={update}
              supporter={supporter.status}
            />
          )}
          {tab === "quiet" && (
            <QuietTab
              settings={settings}
              update={update}
              pauseInfo={pauseInfo}
            />
          )}
          {tab === "system" && (
            <SystemTab
              settings={settings}
              update={update}
              setAutostart={setAutostart}
              hooks={hooks}
            />
          )}
          {tab === "insights" && <InsightsTab stats={stats} />}
          {tab === "profiles" && <ProfilesTab profiles={profiles} />}
          {tab === "about" && <AboutTab supporter={supporter} />}
        </div>
      )}
    </main>
  );
}
