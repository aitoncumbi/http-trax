import { create } from "zustand";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { Branch, Commit, FileStatus, RepoStatus } from "@/lib/types";

export type Selection = { kind: "file"; path: string; staged: boolean } | { kind: "commit"; hash: string };

interface GitState {
  workspaceId: string | null;
  status: RepoStatus | null;
  branches: Branch[];
  log: Commit[];
  busy: string | null;
  selected: Selection | null;
  message: string;
  /** Bumped by refresh() to reload status, branches and history. */
  reloadKey: number;
  setSelected: (s: Selection | null) => void;
  setMessage: (m: string) => void;
  refresh: () => void;
  load: (workspaceId: string) => Promise<void>;
  /** Run a git action with a busy label, then reload everything. */
  run: (label: string, fn: (workspaceId: string) => Promise<unknown>, success?: (v: unknown) => string | null) => Promise<boolean>;
}

/**
 * Source control state shared by the changes list (sidebar) and the
 * diff/history (main card).
 */
export const useGit = create<GitState>((set, get) => ({
  workspaceId: null,
  status: null,
  branches: [],
  log: [],
  busy: null,
  selected: null,
  message: "",
  reloadKey: 0,
  setSelected: (selected) => set({ selected }),
  setMessage: (message) => set({ message }),
  refresh: () => set((s) => ({ reloadKey: s.reloadKey + 1 })),
  load: async (workspaceId) => {
    if (get().workspaceId !== workspaceId) {
      set({ workspaceId, status: null, branches: [], log: [], selected: null, message: "" });
    }
    try {
      const status = await api.gitStatus(workspaceId);
      if (get().workspaceId !== workspaceId) return;
      if (!status.isRepo) return set({ status, branches: [], log: [] });
      const [branches, log] = await Promise.all([api.gitBranches(workspaceId), api.gitLog(workspaceId, 60).catch(() => [] as Commit[])]);
      if (get().workspaceId !== workspaceId) return;
      set({ status, branches, log });
    } catch (e) {
      useStore.getState().toast("error", errorMessage(e));
    }
  },
  run: async (label, fn, success) => {
    const workspaceId = get().workspaceId;
    if (!workspaceId) return false;
    const { toast, reloadTree, bumpGit } = useStore.getState();
    set({ busy: label });
    try {
      const v = await fn(workspaceId);
      const msg = success?.(v);
      if (msg) toast(v && typeof v === "object" && "kind" in v && v.kind === "conflicts" ? "error" : "success", msg);
      await reloadTree();
      bumpGit();
      return true;
    } catch (e) {
      toast("error", errorMessage(e));
      get().refresh();
      return false;
    } finally {
      set({ busy: null });
    }
  },
}));

export interface GitLists {
  files: FileStatus[];
  conflicted: FileStatus[];
  staged: FileStatus[];
  changes: FileStatus[];
}

export function lists(status: RepoStatus | null): GitLists {
  const files = status?.files ?? [];
  return {
    files,
    conflicted: files.filter((f) => f.conflicted),
    staged: files.filter((f) => f.staged && !f.conflicted),
    changes: files.filter((f) => f.unstaged && !f.conflicted),
  };
}

/** The current selection, falling back to the first change while the chosen one is gone. */
export function effectiveSelection(selected: Selection | null, status: RepoStatus | null, log: Commit[]): Selection | null {
  const { files, conflicted, staged, changes } = lists(status);
  if (selected?.kind === "commit" && log.some((c) => c.hash === selected.hash)) return selected;
  if (selected?.kind === "file") {
    const f = files.find((x) => x.path === selected.path);
    if (f && (f.conflicted || (selected.staged ? f.staged : f.unstaged))) return selected;
    if (f) return { kind: "file", path: f.path, staged: !selected.staged };
  }
  if (conflicted[0]) return { kind: "file", path: conflicted[0].path, staged: false };
  if (staged[0]) return { kind: "file", path: staged[0].path, staged: true };
  if (changes[0]) return { kind: "file", path: changes[0].path, staged: false };
  return null;
}
