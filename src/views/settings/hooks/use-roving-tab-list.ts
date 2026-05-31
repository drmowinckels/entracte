import {
  useCallback,
  useEffect,
  useRef,
  type KeyboardEvent,
  type RefCallback,
} from "react";

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
 * Tabs pattern with **automatic activation** (arrow keys + Home/End
 * both move focus and activate the focused tab). Automatic activation
 * is appropriate here because panels are local React state with no
 * load latency; if a future tab ever has to lazy-fetch on activation
 * the call sites should switch to manual activation (Space/Enter to
 * activate after focus lands).
 *
 * Held arrow keys traverse correctly: the hook reads the current
 * active id from a ref that's updated on every render, so successive
 * keydowns before React re-renders still see fresh state.
 *
 * `onChange` does not need to be referentially stable — the hook
 * stashes the latest value in a ref so changing identities don't
 * thrash the tab button ref callbacks.
 */
export function useRovingTabList<T extends string>({
  ids,
  active,
  onChange,
  orientation = "horizontal",
}: Options<T>): Returned<T> {
  const refs = useRef<Map<T, HTMLButtonElement>>(new Map());
  const refCallbacks = useRef<Map<T, RefCallback<HTMLButtonElement>>>(
    new Map(),
  );
  const onChangeRef = useRef(onChange);
  const activeRef = useRef(active);
  useEffect(() => {
    onChangeRef.current = onChange;
    activeRef.current = active;
  });

  const setTabRef = useCallback((id: T): RefCallback<HTMLButtonElement> => {
    const cached = refCallbacks.current.get(id);
    if (cached) return cached;
    const cb: RefCallback<HTMLButtonElement> = (node) => {
      if (node) refs.current.set(id, node);
      else refs.current.delete(id);
    };
    refCallbacks.current.set(id, cb);
    return cb;
  }, []);

  const activate = useCallback((id: T) => {
    onChangeRef.current(id);
    const node = refs.current.get(id);
    if (node) node.focus();
  }, []);

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLElement>) => {
      const prevKey = orientation === "horizontal" ? "ArrowLeft" : "ArrowUp";
      const nextKey = orientation === "horizontal" ? "ArrowRight" : "ArrowDown";
      const current = ids.indexOf(activeRef.current);
      if (current < 0) return;
      const last = ids.length - 1;
      let target: T | null = null;
      if (event.key === prevKey)
        target = ids[current === 0 ? last : current - 1];
      else if (event.key === nextKey)
        target = ids[current === last ? 0 : current + 1];
      else if (event.key === "Home") target = ids[0];
      else if (event.key === "End") target = ids[last];
      if (target === null || target === undefined) return;
      event.preventDefault();
      activeRef.current = target;
      activate(target);
    },
    [activate, ids, orientation],
  );

  const tabProps = (id: T): TabProps => {
    const isActive = id === active;
    return {
      role: "tab",
      "aria-selected": isActive,
      tabIndex: isActive ? 0 : -1,
      ref: setTabRef(id),
      onClick: () => onChangeRef.current(id),
      onFocus: () => {
        if (id !== activeRef.current) onChangeRef.current(id);
      },
    };
  };

  return {
    tablistProps: {
      role: "tablist",
      "aria-orientation": orientation,
      onKeyDown,
    },
    tabProps,
  };
}
