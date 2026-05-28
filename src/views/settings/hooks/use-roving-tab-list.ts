import { useCallback, useRef, type KeyboardEvent, type RefCallback } from "react";

type Orientation = "horizontal" | "vertical";

type Options<T extends string> = {
  ids: readonly T[];
  active: T;
  onChange: (id: T) => void;
  orientation?: Orientation;
};

type TablistProps = {
  role: "tablist";
  "aria-orientation": Orientation;
  onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
};

type TabProps = {
  role: "tab";
  "aria-selected": boolean;
  tabIndex: 0 | -1;
  ref: RefCallback<HTMLButtonElement>;
  onClick: () => void;
  onFocus: () => void;
};

type Returned<T extends string> = {
  tablistProps: TablistProps;
  tabProps: (id: T) => TabProps;
};

/**
 * Roving-tabindex controller for an ARIA tablist. Follows the W3C APG
 * automatic-activation pattern: arrow keys (and Home/End) both move focus
 * across tabs and activate the focused tab, since panels are local React
 * state and have no load latency. Tab from inside the tablist falls
 * through to the next focusable element — typically the active tabpanel
 * with tabIndex={0}.
 */
export function useRovingTabList<T extends string>({
  ids,
  active,
  onChange,
  orientation = "horizontal",
}: Options<T>): Returned<T> {
  const refs = useRef<Map<T, HTMLButtonElement>>(new Map());

  const setTabRef = useCallback(
    (id: T): RefCallback<HTMLButtonElement> =>
      (node) => {
        if (node) refs.current.set(id, node);
        else refs.current.delete(id);
      },
    [],
  );

  const activate = useCallback(
    (id: T) => {
      onChange(id);
      const node = refs.current.get(id);
      if (node) node.focus();
    },
    [onChange],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLElement>) => {
      const prevKey = orientation === "horizontal" ? "ArrowLeft" : "ArrowUp";
      const nextKey = orientation === "horizontal" ? "ArrowRight" : "ArrowDown";
      const current = ids.indexOf(active);
      if (current < 0) return;
      const last = ids.length - 1;
      let target: T | null = null;
      if (event.key === prevKey) target = ids[current === 0 ? last : current - 1];
      else if (event.key === nextKey) target = ids[current === last ? 0 : current + 1];
      else if (event.key === "Home") target = ids[0];
      else if (event.key === "End") target = ids[last];
      if (target === null || target === undefined) return;
      event.preventDefault();
      activate(target);
    },
    [activate, active, ids, orientation],
  );

  const tabProps = useCallback(
    (id: T): TabProps => {
      const isActive = id === active;
      return {
        role: "tab",
        "aria-selected": isActive,
        tabIndex: isActive ? 0 : -1,
        ref: setTabRef(id),
        onClick: () => onChange(id),
        onFocus: () => {
          if (!isActive) onChange(id);
        },
      };
    },
    [active, onChange, setTabRef],
  );

  return {
    tablistProps: {
      role: "tablist",
      "aria-orientation": orientation,
      onKeyDown,
    },
    tabProps,
  };
}
