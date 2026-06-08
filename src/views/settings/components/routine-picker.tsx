import type { Routine } from "../hooks/use-routines";
import type { UseSettings } from "../hooks/use-settings";
import type {
  RoutineCategory,
  RoutineDifficulty,
  SchedulerSettings,
} from "../types";
import { InfoTip } from "./info-tip";

const CATEGORIES: { id: RoutineCategory; label: string }[] = [
  { id: "eyes", label: "Eyes" },
  { id: "mobility", label: "Mobility" },
  { id: "breathing", label: "Breathing" },
  { id: "desk_yoga", label: "Desk yoga" },
];

const DIFFICULTIES: { id: RoutineDifficulty; label: string }[] = [
  { id: "gentle", label: "Gentle" },
  { id: "moderate", label: "Moderate" },
  { id: "active", label: "Active" },
];

type RoutineKey = "micro_routine" | "long_routine";
type CategoriesKey = "micro_routine_categories" | "long_routine_categories";
type DifficultyKey =
  | "micro_routine_max_difficulty"
  | "long_routine_max_difficulty";

// Per-kind guided-routine picker. Three modes: None (rotate ideas), a
// specific routine, or Random — which reveals the engine filters (categories
// + max difficulty) that the backend draws the per-break routine from.
export function RoutinePicker({
  kind,
  routineKey,
  categoriesKey,
  difficultyKey,
  settings,
  update,
  routines,
}: {
  kind: "micro" | "long";
  routineKey: RoutineKey;
  categoriesKey: CategoriesKey;
  difficultyKey: DifficultyKey;
  settings: SchedulerSettings;
  update: UseSettings["update"];
  routines: Routine[];
}) {
  const mode = settings[routineKey];
  const selectedCategories = settings[categoriesKey];

  const toggleCategory = (cat: RoutineCategory) => {
    const next = selectedCategories.includes(cat)
      ? selectedCategories.filter((c) => c !== cat)
      : [...selectedCategories, cat];
    update(categoriesKey, next);
  };

  return (
    <>
      <label className="row">
        <span>
          Guided routine
          <InfoTip text="Step-by-step prompts that advance through the break instead of a single rotating idea. Random picks a fresh routine each break from the filters below; None keeps the rotating ideas above." />
        </span>
        <select
          value={mode}
          onChange={(e) => update(routineKey, e.target.value)}
        >
          <option value="">None (rotate ideas)</option>
          <option value="random">Random (from filters)</option>
          {routines
            .filter((r) => r.kind === kind)
            .map((r) => (
              <option key={r.id} value={r.id}>
                {r.label}
              </option>
            ))}
        </select>
      </label>
      {mode === "random" && (
        <>
          <div className="row routine-filter">
            <span>
              Categories
              <InfoTip text="Draw routines only from the ticked categories. Leave all unticked to draw from every category." />
            </span>
            <span className="routine-categories">
              {CATEGORIES.map((cat) => (
                <label key={cat.id} className="routine-category">
                  <input
                    type="checkbox"
                    checked={selectedCategories.includes(cat.id)}
                    onChange={() => toggleCategory(cat.id)}
                  />
                  <span>{cat.label}</span>
                </label>
              ))}
            </span>
          </div>
          <label className="row">
            <span>
              Maximum difficulty
              <InfoTip text="Include routines up to and including this level — Gentle for the lightest only, Active for everything." />
            </span>
            <select
              value={settings[difficultyKey]}
              onChange={(e) =>
                update(difficultyKey, e.target.value as RoutineDifficulty)
              }
            >
              {DIFFICULTIES.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.label}
                </option>
              ))}
            </select>
          </label>
        </>
      )}
    </>
  );
}
