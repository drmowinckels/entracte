import { useCallback, useEffect, useRef, useState } from "react";
import { z } from "zod";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { invoke } from "../../../lib/ipc";
import { useTauriListen } from "../../../lib/use-tauri-listen";

const profilesListSchema = z.array(z.string());
const activeProfileSchema = z.string();

const CONFIRM_TIMEOUT_MS = 4000;

/** All state + actions the Profiles tab needs. Action methods surface
 * failures through `profileError` rather than throwing, so the UI can
 * show inline messages without try/catch. */
export type UseProfiles = {
  profiles: string[];
  activeProfile: string;
  newProfileName: string;
  setNewProfileName: (name: string) => void;
  renameDrafts: Record<string, string>;
  setRenameDraft: (name: string, draft: string | null) => void;
  deleteCandidate: string | null;
  resetCandidate: string | null;
  profileError: string;
  refresh: () => Promise<void>;
  switchTo: (name: string) => Promise<void>;
  create: () => Promise<void>;
  duplicate: (source: string) => Promise<void>;
  rename: (from: string) => Promise<void>;
  requestDelete: (name: string) => void;
  confirmDelete: (name: string) => Promise<void>;
  requestReset: (name: string) => void;
  confirmReset: (name: string) => Promise<void>;
  move: (name: string, delta: -1 | 1) => Promise<void>;
};

/** Owns the profile list + active-profile pointer + the two-step
 * (request → confirm) destructive-action UI. Subscribes to
 * `profile:changed` so background changes (tray menu, CLI, IPC) keep
 * the UI in sync. */
export function useProfiles(): UseProfiles {
  const [profiles, setProfiles] = useState<string[]>([]);
  const [activeProfile, setActiveProfile] = useState("");
  const [newProfileName, setNewProfileName] = useState("");
  const [renameDrafts, setRenameDrafts] = useState<Record<string, string>>({});
  const [deleteCandidate, setDeleteCandidate] = useState<string | null>(null);
  const [resetCandidate, setResetCandidate] = useState<string | null>(null);
  const [profileError, setProfileError] = useState("");
  const deleteTimeoutRef = useRef<number | null>(null);
  const resetTimeoutRef = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [list, active] = await Promise.all([
        invoke("list_profiles", undefined, profilesListSchema),
        invoke("get_active_profile", undefined, activeProfileSchema),
      ]);
      setProfiles(list);
      setActiveProfile(active);
    } catch (e) {
      console.error("profile fetch failed", e);
    }
  }, []);

  useEffect(() => {
    refresh();
    return () => {
      if (deleteTimeoutRef.current !== null)
        window.clearTimeout(deleteTimeoutRef.current);
      if (resetTimeoutRef.current !== null)
        window.clearTimeout(resetTimeoutRef.current);
    };
  }, [refresh]);

  useTauriListen(
    "profile:changed",
    () => {
      refresh();
    },
    [refresh],
  );

  const setRenameDraft = useCallback((name: string, draft: string | null) => {
    setRenameDrafts((d) => {
      const next = { ...d };
      if (draft === null) delete next[name];
      else next[name] = draft;
      return next;
    });
  }, []);

  const switchTo = useCallback(
    async (name: string) => {
      if (name === activeProfile) return;
      try {
        setProfileError("");
        await tauriInvoke("set_active_profile", { name });
      } catch (e) {
        setProfileError(String(e));
      }
    },
    [activeProfile],
  );

  const create = useCallback(async () => {
    const name = newProfileName.trim();
    if (!name) return;
    try {
      setProfileError("");
      await tauriInvoke("create_profile", { name });
      setNewProfileName("");
      await refresh();
    } catch (e) {
      setProfileError(String(e));
    }
  }, [newProfileName, refresh]);

  const duplicate = useCallback(
    async (source: string) => {
      const base = `${source} copy`;
      let candidate = base;
      let i = 2;
      while (profiles.includes(candidate)) {
        candidate = `${base} ${i}`;
        i += 1;
      }
      try {
        setProfileError("");
        // Server-side duplicate: one IPC call, doesn't flip the active
        // profile. The previous switch+create pattern triggered
        // profile:changed mid-operation and clobbered hook drafts.
        await tauriInvoke("duplicate_profile", { source, name: candidate });
        await refresh();
      } catch (e) {
        setProfileError(String(e));
      }
    },
    [profiles, refresh],
  );

  const rename = useCallback(
    async (from: string) => {
      const to = (renameDrafts[from] ?? "").trim();
      if (!to || to === from) {
        setRenameDraft(from, null);
        return;
      }
      try {
        setProfileError("");
        await tauriInvoke("rename_profile", { from, to });
        setRenameDraft(from, null);
        await refresh();
      } catch (e) {
        setProfileError(String(e));
      }
    },
    [renameDrafts, setRenameDraft, refresh],
  );

  const requestDelete = useCallback((name: string) => {
    setProfileError("");
    setDeleteCandidate(name);
    if (deleteTimeoutRef.current !== null)
      window.clearTimeout(deleteTimeoutRef.current);
    deleteTimeoutRef.current = window.setTimeout(() => {
      setDeleteCandidate((cur) => (cur === name ? null : cur));
      deleteTimeoutRef.current = null;
    }, CONFIRM_TIMEOUT_MS);
  }, []);

  const confirmDelete = useCallback(
    async (name: string) => {
      setDeleteCandidate(null);
      try {
        setProfileError("");
        await tauriInvoke("delete_profile", { name });
        await refresh();
      } catch (e) {
        setProfileError(String(e));
      }
    },
    [refresh],
  );

  const requestReset = useCallback((name: string) => {
    setProfileError("");
    setResetCandidate(name);
    if (resetTimeoutRef.current !== null)
      window.clearTimeout(resetTimeoutRef.current);
    resetTimeoutRef.current = window.setTimeout(() => {
      setResetCandidate((cur) => (cur === name ? null : cur));
      resetTimeoutRef.current = null;
    }, CONFIRM_TIMEOUT_MS);
  }, []);

  const confirmReset = useCallback(
    async (name: string) => {
      setResetCandidate(null);
      try {
        setProfileError("");
        await tauriInvoke("reset_profile_to_defaults", { name });
        await refresh();
      } catch (e) {
        setProfileError(String(e));
      }
    },
    [refresh],
  );

  const move = useCallback(
    async (name: string, delta: -1 | 1) => {
      const idx = profiles.indexOf(name);
      const target = idx + delta;
      if (idx === -1 || target < 0 || target >= profiles.length) return;
      const next = [...profiles];
      [next[idx], next[target]] = [next[target], next[idx]];
      try {
        setProfileError("");
        setProfiles(next);
        await tauriInvoke("reorder_profiles", { names: next });
      } catch (e) {
        setProfileError(String(e));
        await refresh();
      }
    },
    [profiles, refresh],
  );

  return {
    profiles,
    activeProfile,
    newProfileName,
    setNewProfileName,
    renameDrafts,
    setRenameDraft,
    deleteCandidate,
    resetCandidate,
    profileError,
    refresh,
    switchTo,
    create,
    duplicate,
    rename,
    requestDelete,
    confirmDelete,
    requestReset,
    confirmReset,
    move,
  };
}
