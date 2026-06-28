import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// happy-dom doesn't expose a `localStorage`, but the WebView the app runs in
// does. Install a minimal in-memory stand-in so code that persists to it (e.g.
// the language choice) behaves as it does in the real app. Defined rather than
// read-then-assigned, so it never trips Node's experimental-localStorage probe.
const localStorageStore = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    getItem: (key: string) =>
      localStorageStore.has(key)
        ? (localStorageStore.get(key) as string)
        : null,
    setItem: (key: string, value: string) => {
      localStorageStore.set(key, String(value));
    },
    removeItem: (key: string) => {
      localStorageStore.delete(key);
    },
    clear: () => {
      localStorageStore.clear();
    },
    key: (index: number) => Array.from(localStorageStore.keys())[index] ?? null,
    get length() {
      return localStorageStore.size;
    },
  } as Storage,
});

afterEach(() => {
  cleanup();
});
