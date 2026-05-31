import {
  useEffect,
  useRef,
  useState,
  type DependencyList,
  type Dispatch,
  type SetStateAction,
} from "react";

/**
 * Editable local copy of a value derived from upstream state (typically
 * a setting), re-seeded whenever `deps` change — e.g. when the active
 * profile swaps the underlying setting out from under an open textarea.
 *
 * `compute` produces both the initial draft and each re-seed. It is read
 * through a ref so callers can pass a fresh closure every render without
 * having to add it to `deps`.
 */
export function useLocalDraft<T>(
  compute: () => T,
  deps: DependencyList,
): [T, Dispatch<SetStateAction<T>>] {
  const computeRef = useRef(compute);
  computeRef.current = compute;
  const [draft, setDraft] = useState(compute);
  useEffect(() => {
    setDraft(computeRef.current());
    // `compute` is intentionally excluded — it is read from a ref so the
    // re-seed tracks only the caller-supplied `deps`.
  }, deps);
  return [draft, setDraft];
}
