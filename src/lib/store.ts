"use client";

import { create } from "zustand";

import { api, errorMessage, onEvent } from "./api";
import type {
  AppInfo,
  CollectionRun,
  GroupMeta,
  Request,
  Review,
  RunResult,
  SavedResponse,
  SyncReport,
  WorkspaceEntry,
  WorkspacePrefs,
  WorkspaceTree,
} from "./types";
import { findChain, newRequest, normaliseRequest, stableStringify, uid } from "./util";

// --------------------------------------------------------------------- tabs

interface TabBase {
  id: string;
  workspaceId: string;
}

export interface RequestTab extends TabBase {
  kind: "request";
  requestId: string;
  draft: Request;
  /** Serialised version last loaded from / saved to disk. */
  base: string;
  /** The file changed on disk while this tab had unsaved edits. */
  diskVersion: Request | null;
  result: RunResult | null;
  running: boolean;
  saved: SavedResponse[];
  viewingSaved: string | null;
}

export interface GroupTab extends TabBase {
  kind: "group";
  groupId: string;
  isCollection: boolean;
  draft: GroupMeta;
  base: string;
}

export interface RunnerTab extends TabBase {
  kind: "runner";
  groupId: string;
  groupName: string;
  environmentId: string | null;
  run: CollectionRun | null;
  running: boolean;
  error: string | null;
}

export interface ReviewTab extends TabBase {
  kind: "review";
  base: string;
  head: string;
  review: Review | null;
  loading: boolean;
  error: string | null;
}

export interface DiffTab extends TabBase {
  kind: "diff";
  title: string;
  text: string;
}

export type Tab = RequestTab | GroupTab | RunnerTab | ReviewTab | DiffTab;

export type View = "collections" | "environments" | "history" | "git" | "mcp" | "settings";

export type PaneLayout = "stack" | "side";

export type PaletteMode = "search" | "workspaces" | "environments";

export interface Toast {
  id: string;
  kind: "info" | "success" | "error";
  message: string;
}

export type Dialog =
  | { kind: "prompt"; title: string; label?: string; initial?: string; confirm?: string; resolve: (v: string | null) => void }
  | { kind: "confirm"; title: string; message: string; confirm?: string; danger?: boolean; resolve: (v: boolean) => void }
  | { kind: "workspace"; mode: "create" | "open" | "clone" }
  | { kind: "import"; tab?: "curl" | "postman" };

export function tabDirty(tab: Tab): boolean {
  switch (tab.kind) {
    case "request":
    case "group":
      return stableStringify(tab.draft) !== tab.base;
    default:
      return false;
  }
}

interface State {
  ready: boolean;
  info: AppInfo | null;
  workspaces: WorkspaceEntry[];
  activeWorkspaceId: string | null;
  tree: WorkspaceTree | null;
  treeError: string | null;
  sync: Record<string, SyncReport>;
  syncing: boolean;
  view: View;
  tabs: Tab[];
  activeTab: Record<string, string | null>;
  toasts: Toast[];
  dialog: Dialog | null;
  /** Bumped whenever git state may have changed (events, own actions). */
  gitEpoch: number;
  /** Bumped after runs so the History panel reloads. */
  historyEpoch: number;
  theme: "dark" | "light";
  layout: PaneLayout;
  palette: PaletteMode | null;
}

interface Actions {
  init: () => Promise<void>;
  refreshWorkspaces: () => Promise<void>;
  selectWorkspace: (id: string) => Promise<void>;
  reloadTree: () => Promise<void>;
  setView: (view: View) => void;
  toggleTheme: () => void;

  activeEntry: () => WorkspaceEntry | null;
  updatePrefs: (fn: (p: WorkspacePrefs) => WorkspacePrefs) => Promise<void>;
  toggleBookmark: (collectionId: string) => Promise<void>;
  setScope: (collectionId: string | null) => Promise<void>;
  setActiveEnvironment: (envId: string | null) => Promise<void>;

  openRequest: (requestId: string) => Promise<void>;
  createRequest: (parentId: string) => Promise<void>;
  createCollection: () => Promise<void>;
  createFolder: (parentId: string) => Promise<void>;
  renameGroup: (groupId: string) => Promise<void>;
  deleteGroup: (groupId: string) => Promise<void>;
  deleteRequest: (requestId: string) => Promise<void>;
  duplicateRequest: (requestId: string) => Promise<void>;
  openGroup: (groupId: string) => Promise<void>;
  openEnvironment: (envId: string) => void;
  createEnvironment: () => Promise<void>;

  updateTab: <T extends Tab>(tabId: string, fn: (tab: T) => T) => void;
  focusTab: (tabId: string) => void;
  closeTab: (tabId: string) => Promise<void>;
  saveTab: (tabId: string) => Promise<void>;
  sendTab: (tabId: string) => Promise<void>;
  reloadTabFromDisk: (tabId: string) => void;

  saveResponse: (tabId: string) => Promise<void>;
  deleteSavedResponse: (tabId: string, responseId: string) => Promise<void>;

  runGroup: (groupId: string) => Promise<void>;
  rerun: (tabId: string) => Promise<void>;
  openReview: (base: string, head: string) => Promise<void>;
  openDiff: (title: string, text: string) => void;

  syncNow: () => Promise<void>;
  onWorkspaceChanged: (workspaceId: string) => Promise<void>;
  bumpGit: () => void;
  setLayout: (layout: PaneLayout) => void;
  openPalette: (mode?: PaletteMode) => void;
  closePalette: () => void;
  /** New request next to the open one (or in the first collection). */
  newRequestHere: () => Promise<void>;
  /** Import a request into a collection or folder and open it. */
  importRequest: (parentId: string, request: Request) => Promise<void>;
  selectWorkspaceByIndex: (index: number) => Promise<void>;

  toast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: string) => void;
  prompt: (opts: { title: string; label?: string; initial?: string; confirm?: string }) => Promise<string | null>;
  confirm: (opts: { title: string; message: string; confirm?: string; danger?: boolean }) => Promise<boolean>;
  showWorkspaceDialog: (mode: "create" | "open" | "clone") => void;
  showImportDialog: (tab?: "curl" | "postman") => void;
  closeDialog: () => void;
}

export type Store = State & Actions;

const THEME_KEY = "http-trax.theme";
const LAYOUT_KEY = "http-trax.layout";

function readLayout(): PaneLayout {
  try {
    return localStorage.getItem(LAYOUT_KEY) === "side" ? "side" : "stack";
  } catch {
    return "stack";
  }
}

function readTheme(): "dark" | "light" {
  try {
    const t = localStorage.getItem(THEME_KEY);
    if (t === "dark" || t === "light") return t;
  } catch {
    // storage unavailable
  }
  if (typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: light)").matches) return "light";
  return "dark";
}

let unsubscribers: (() => void)[] = [];

export const useStore = create<Store>()((set, get) => {
  const fail = (e: unknown) => get().toast("error", errorMessage(e));
  const wsId = () => get().activeWorkspaceId;

  // The source control view covers the editor, so showing a tab leaves it.
  const editorView = (view: View): View => (view === "git" ? "collections" : view);

  const pushTab = (tab: Tab) =>
    set((s) => ({ tabs: [...s.tabs, tab], activeTab: { ...s.activeTab, [tab.workspaceId]: tab.id }, view: editorView(s.view) }));

  const findTab = (pred: (t: Tab) => boolean) => get().tabs.find(pred);

  /** Protected environments (production) need a yes before anything is sent. */
  const confirmProtected = async (envId: string | null, action: string): Promise<boolean> => {
    const env = envId ? get().tree?.environments.find((e) => e.id === envId) : undefined;
    if (!env?.protected) return true;
    return get().confirm({
      title: `${action} with ${env.name}?`,
      message: `${env.name} is a protected environment. Requests go to ${
        env.overrides.baseUrl ?? env.variables.find((v) => v.key === "baseUrl")?.value ?? "its servers"
      }.`,
      confirm: "Send",
      danger: true,
    });
  };

  const loadSaved = async (tabId: string) => {
    const tab = findTab((t) => t.id === tabId) as RequestTab | undefined;
    if (!tab) return;
    try {
      const saved = await api.listSavedResponses(tab.workspaceId, tab.requestId);
      get().updateTab<RequestTab>(tabId, (t) => ({ ...t, saved }));
    } catch {
      // not fatal
    }
  };

  return {
    ready: false,
    info: null,
    workspaces: [],
    activeWorkspaceId: null,
    tree: null,
    treeError: null,
    sync: {},
    syncing: false,
    view: "collections",
    tabs: [],
    activeTab: {},
    toasts: [],
    dialog: null,
    gitEpoch: 0,
    historyEpoch: 0,
    theme: "dark",
    layout: "stack",
    palette: null,

    init: async () => {
      const theme = readTheme();
      document.documentElement.dataset.theme = theme;
      set({ theme, layout: readLayout() });
      try {
        const [info] = await Promise.all([api.appInfo(), get().refreshWorkspaces()]);
        set({ info });
        const sync = await api.syncStatuses().catch(() => ({}));
        set({ sync });
      } catch (e) {
        fail(e);
      }
      unsubscribers.forEach((u) => u());
      unsubscribers = await Promise.all([
        onEvent<{ workspaceId: string; report: SyncReport }>("sync-status", ({ workspaceId, report }) => {
          set((s) => ({ sync: { ...s.sync, [workspaceId]: report } }));
          if (workspaceId === wsId()) get().bumpGit();
        }),
        onEvent<{ workspaceId: string; reason: string }>("workspace-changed", ({ workspaceId, reason }) => {
          if (reason === "pulled" && workspaceId === wsId()) {
            const report = get().sync[workspaceId];
            get().toast("info", `Pulled ${report?.pulled ?? "new"} commit(s) from your team`);
          }
          void get().onWorkspaceChanged(workspaceId);
        }),
        onEvent("registry-changed", () => void get().refreshWorkspaces()),
      ]);
      set({ ready: true });
    },

    refreshWorkspaces: async () => {
      const { workspaces, activeWorkspace } = await api.listWorkspaces();
      const current = wsId();
      const keep = current && workspaces.some((w) => w.id === current) ? current : activeWorkspace;
      set({ workspaces, activeWorkspaceId: keep ?? null });
      if (keep && (keep !== current || !get().tree)) {
        await get().reloadTree();
      } else if (!keep) {
        set({ tree: null });
      }
    },

    selectWorkspace: async (id) => {
      set({ activeWorkspaceId: id, tree: null });
      try {
        await api.setActiveWorkspace(id);
      } catch (e) {
        fail(e);
      }
      await get().reloadTree();
      get().bumpGit();
    },

    reloadTree: async () => {
      const id = wsId();
      if (!id) return;
      try {
        const tree = await api.loadWorkspace(id);
        if (wsId() === id) set({ tree, treeError: null });
      } catch (e) {
        set({ treeError: errorMessage(e) });
      }
    },

    setView: (view) => set({ view }),

    toggleTheme: () => {
      const theme = get().theme === "dark" ? "light" : "dark";
      document.documentElement.dataset.theme = theme;
      try {
        localStorage.setItem(THEME_KEY, theme);
      } catch {
        // storage unavailable
      }
      set({ theme });
    },

    activeEntry: () => get().workspaces.find((w) => w.id === wsId()) ?? null,

    updatePrefs: async (fn) => {
      const entry = get().activeEntry();
      if (!entry) return;
      const prefs = fn(entry.prefs);
      set((s) => ({ workspaces: s.workspaces.map((w) => (w.id === entry.id ? { ...w, prefs } : w)) }));
      try {
        await api.updateWorkspacePrefs(entry.id, prefs);
      } catch (e) {
        fail(e);
      }
    },

    toggleBookmark: (collectionId) =>
      get().updatePrefs((p) => ({
        ...p,
        bookmarks: p.bookmarks.includes(collectionId)
          ? p.bookmarks.filter((b) => b !== collectionId)
          : [...p.bookmarks, collectionId],
      })),

    setScope: (collectionId) => get().updatePrefs((p) => ({ ...p, scopedCollection: collectionId })),

    setActiveEnvironment: (envId) => get().updatePrefs((p) => ({ ...p, activeEnvironment: envId })),

    openRequest: async (requestId) => {
      const id = wsId();
      if (!id) return;
      const existing = findTab((t) => t.kind === "request" && t.requestId === requestId && t.workspaceId === id);
      if (existing) return get().focusTab(existing.id);
      try {
        const request = normaliseRequest(await api.getRequest(id, requestId));
        const tab: RequestTab = {
          kind: "request",
          id: uid(),
          workspaceId: id,
          requestId,
          draft: request,
          base: stableStringify(request),
          diskVersion: null,
          result: null,
          running: false,
          saved: [],
          viewingSaved: null,
        };
        pushTab(tab);
        void loadSaved(tab.id);
      } catch (e) {
        fail(e);
      }
    },

    createRequest: async (parentId) => {
      const id = wsId();
      if (!id) return;
      try {
        const created = await api.createRequest(id, parentId, newRequest());
        await get().reloadTree();
        await get().openRequest(created.id);
      } catch (e) {
        fail(e);
      }
    },

    createCollection: async () => {
      const id = wsId();
      if (!id) return;
      const name = await get().prompt({ title: "New collection", label: "Name", confirm: "Create" });
      if (!name) return;
      try {
        await api.createCollection(id, name);
        await get().reloadTree();
      } catch (e) {
        fail(e);
      }
    },

    createFolder: async (parentId) => {
      const id = wsId();
      if (!id) return;
      const name = await get().prompt({ title: "New folder", label: "Name", confirm: "Create" });
      if (!name) return;
      try {
        await api.createFolder(id, parentId, name);
        await get().reloadTree();
      } catch (e) {
        fail(e);
      }
    },

    renameGroup: async (groupId) => {
      const id = wsId();
      if (!id) return;
      try {
        const meta = await api.getGroup(id, groupId);
        const name = await get().prompt({ title: "Rename", label: "Name", initial: meta.name, confirm: "Rename" });
        if (!name || name === meta.name) return;
        await api.updateGroup(id, { ...meta, name });
        await get().reloadTree();
      } catch (e) {
        fail(e);
      }
    },

    deleteGroup: async (groupId) => {
      const id = wsId();
      const chain = findChain(get().tree?.collections ?? [], groupId);
      const node = chain?.[chain.length - 1];
      if (!id || !node) return;
      const ok = await get().confirm({
        title: `Delete "${node.meta.name}"?`,
        message: "Its requests and saved responses are deleted from disk. You can still recover them from git history if they were committed.",
        confirm: "Delete",
        danger: true,
      });
      if (!ok) return;
      try {
        await api.deleteGroup(id, groupId);
        await get().refreshWorkspaces();
        await get().reloadTree();
      } catch (e) {
        fail(e);
      }
    },

    deleteRequest: async (requestId) => {
      const id = wsId();
      if (!id) return;
      const ok = await get().confirm({ title: "Delete request?", message: "The request file and its saved responses are removed.", confirm: "Delete", danger: true });
      if (!ok) return;
      try {
        await api.deleteRequest(id, requestId);
        set((s) => ({ tabs: s.tabs.filter((t) => !(t.kind === "request" && t.requestId === requestId)) }));
        await get().reloadTree();
      } catch (e) {
        fail(e);
      }
    },

    duplicateRequest: async (requestId) => {
      const id = wsId();
      if (!id) return;
      try {
        const copy = await api.duplicateRequest(id, requestId);
        await get().reloadTree();
        await get().openRequest(copy.id);
      } catch (e) {
        fail(e);
      }
    },

    openGroup: async (groupId) => {
      const id = wsId();
      if (!id) return;
      const existing = findTab((t) => t.kind === "group" && t.groupId === groupId && t.workspaceId === id);
      if (existing) return get().focusTab(existing.id);
      try {
        const meta = await api.getGroup(id, groupId);
        const draft: GroupMeta = {
          ...meta,
          description: meta.description ?? "",
          variables: meta.variables ?? [],
          headers: meta.headers ?? [],
          auth: meta.auth ?? { type: "inherit" },
          scripts: { preRequest: meta.scripts?.preRequest ?? "", postResponse: meta.scripts?.postResponse ?? "" },
        };
        const isCollection = (get().tree?.collections ?? []).some((c) => c.meta.id === groupId);
        pushTab({ kind: "group", id: uid(), workspaceId: id, groupId, isCollection, draft, base: stableStringify(draft) });
      } catch (e) {
        fail(e);
      }
    },

    // Environments are edited in the Environments view; picking one there
    // also makes it the active one (what you see is what requests use).
    openEnvironment: (envId) => {
      if (get().tree?.environments.some((e) => e.id === envId)) void get().setActiveEnvironment(envId);
      set({ view: "environments" });
    },

    createEnvironment: async () => {
      const id = wsId();
      if (!id) return;
      const name = await get().prompt({ title: "New environment", label: "Name", initial: "Local", confirm: "Create" });
      if (!name) return;
      try {
        const env = await api.saveEnvironment(id, {
          id: "",
          name,
          variables: [{ key: "baseUrl", value: "http://localhost:8080", enabled: true }],
        });
        await get().reloadTree();
        if (!get().activeEntry()?.prefs.activeEnvironment) await get().setActiveEnvironment(env.id);
        get().openEnvironment(env.id);
      } catch (e) {
        fail(e);
      }
    },

    updateTab: (tabId, fn) =>
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === tabId ? (fn(t as never) as Tab) : t)) })),

    focusTab: (tabId) => {
      const tab = findTab((t) => t.id === tabId);
      if (tab) set((s) => ({ activeTab: { ...s.activeTab, [tab.workspaceId]: tabId }, view: editorView(s.view) }));
    },

    closeTab: async (tabId) => {
      const tab = findTab((t) => t.id === tabId);
      if (!tab) return;
      if (tabDirty(tab)) {
        const ok = await get().confirm({ title: "Discard unsaved changes?", message: "This tab has changes that haven't been saved.", confirm: "Discard", danger: true });
        if (!ok) return;
      }
      set((s) => {
        const siblings = s.tabs.filter((t) => t.workspaceId === tab.workspaceId);
        const index = siblings.findIndex((t) => t.id === tabId);
        const remaining = siblings.filter((t) => t.id !== tabId);
        const nextActive =
          s.activeTab[tab.workspaceId] === tabId
            ? (remaining[Math.min(index, remaining.length - 1)]?.id ?? null)
            : s.activeTab[tab.workspaceId];
        return { tabs: s.tabs.filter((t) => t.id !== tabId), activeTab: { ...s.activeTab, [tab.workspaceId]: nextActive } };
      });
    },

    saveTab: async (tabId) => {
      const tab = findTab((t) => t.id === tabId);
      if (!tab) return;
      try {
        if (tab.kind === "request") {
          await api.saveRequest(tab.workspaceId, tab.draft);
          get().updateTab<RequestTab>(tabId, (t) => ({ ...t, base: stableStringify(t.draft), diskVersion: null }));
        } else if (tab.kind === "group") {
          await api.updateGroup(tab.workspaceId, tab.draft);
          get().updateTab<GroupTab>(tabId, (t) => ({ ...t, base: stableStringify(t.draft) }));
        } else {
          return;
        }
        await get().reloadTree();
        get().bumpGit();
      } catch (e) {
        fail(e);
      }
    },

    sendTab: async (tabId) => {
      const tab = findTab((t) => t.id === tabId) as RequestTab | undefined;
      if (!tab || tab.kind !== "request" || tab.running) return;
      const env = get().workspaces.find((w) => w.id === tab.workspaceId)?.prefs.activeEnvironment ?? null;
      if (!(await confirmProtected(env, `Send ${tab.draft.name}`))) return;
      get().updateTab<RequestTab>(tabId, (t) => ({ ...t, running: true, viewingSaved: null }));
      try {
        const result = await api.sendRequest(tab.workspaceId, tab.draft, null, env);
        get().updateTab<RequestTab>(tabId, (t) => ({ ...t, result, running: false }));
        set((st) => ({ historyEpoch: st.historyEpoch + 1 }));
        // Scripts may have changed environment values.
        void get().reloadTree();
      } catch (e) {
        get().updateTab<RequestTab>(tabId, (t) => ({ ...t, running: false }));
        fail(e);
      }
    },

    reloadTabFromDisk: (tabId) =>
      get().updateTab<RequestTab>(tabId, (t) =>
        t.diskVersion ? { ...t, draft: t.diskVersion, base: stableStringify(t.diskVersion), diskVersion: null } : t,
      ),

    saveResponse: async (tabId) => {
      const tab = findTab((t) => t.id === tabId) as RequestTab | undefined;
      if (!tab?.result?.response) return;
      const suggested = `${tab.result.response.status} ${tab.result.response.statusText}`.trim();
      const name = await get().prompt({ title: "Save response", label: "Name", initial: suggested, confirm: "Save" });
      if (name === null) return;
      try {
        await api.saveResponse(tab.workspaceId, tab.requestId, name, tab.result);
        await loadSaved(tabId);
        get().toast("success", "Response saved");
        get().bumpGit();
      } catch (e) {
        fail(e);
      }
    },

    deleteSavedResponse: async (tabId, responseId) => {
      const tab = findTab((t) => t.id === tabId) as RequestTab | undefined;
      if (!tab) return;
      try {
        await api.deleteSavedResponse(tab.workspaceId, tab.requestId, responseId);
        get().updateTab<RequestTab>(tabId, (t) => ({
          ...t,
          viewingSaved: t.viewingSaved === responseId ? null : t.viewingSaved,
        }));
        await loadSaved(tabId);
      } catch (e) {
        fail(e);
      }
    },

    runGroup: async (groupId) => {
      const id = wsId();
      const chain = findChain(get().tree?.collections ?? [], groupId);
      const node = chain?.[chain.length - 1];
      if (!id || !node) return;
      const environmentId = get().activeEntry()?.prefs.activeEnvironment ?? null;
      if (!(await confirmProtected(environmentId, `Run ${node.meta.name}`))) return;
      const tab: RunnerTab = {
        kind: "runner",
        id: uid(),
        workspaceId: id,
        groupId,
        groupName: node.meta.name,
        environmentId,
        run: null,
        running: true,
        error: null,
      };
      pushTab(tab);
      await get().rerun(tab.id);
    },

    rerun: async (tabId) => {
      const tab = findTab((t) => t.id === tabId) as RunnerTab | undefined;
      if (!tab) return;
      get().updateTab<RunnerTab>(tabId, (t) => ({ ...t, running: true, error: null }));
      try {
        const run = await api.runCollection(tab.workspaceId, tab.groupId, tab.environmentId, false);
        get().updateTab<RunnerTab>(tabId, (t) => ({ ...t, run, running: false }));
        set((st) => ({ historyEpoch: st.historyEpoch + 1 }));
        void get().reloadTree();
      } catch (e) {
        get().updateTab<RunnerTab>(tabId, (t) => ({ ...t, running: false, error: errorMessage(e) }));
      }
    },

    openReview: async (base, head) => {
      const id = wsId();
      if (!id) return;
      const tab: ReviewTab = { kind: "review", id: uid(), workspaceId: id, base, head, review: null, loading: true, error: null };
      pushTab(tab);
      try {
        const review = await api.gitReview(id, base, head);
        get().updateTab<ReviewTab>(tab.id, (t) => ({ ...t, review, loading: false }));
      } catch (e) {
        get().updateTab<ReviewTab>(tab.id, (t) => ({ ...t, loading: false, error: errorMessage(e) }));
      }
    },

    openDiff: (title, text) => {
      const id = wsId();
      if (!id) return;
      const existing = findTab((t) => t.kind === "diff" && t.title === title && t.workspaceId === id) as DiffTab | undefined;
      if (existing) {
        get().updateTab<DiffTab>(existing.id, (t) => ({ ...t, text }));
        return get().focusTab(existing.id);
      }
      pushTab({ kind: "diff", id: uid(), workspaceId: id, title, text });
    },

    syncNow: async () => {
      const id = wsId();
      if (!id || get().syncing) return;
      set({ syncing: true });
      try {
        const report = await api.syncNow(id);
        if (report) set((s) => ({ sync: { ...s.sync, [id]: report } }));
        get().bumpGit();
        await get().reloadTree();
      } catch (e) {
        fail(e);
      } finally {
        set({ syncing: false });
      }
    },

    onWorkspaceChanged: async (workspaceId) => {
      get().bumpGit();
      if (workspaceId !== wsId()) return;
      await get().reloadTree();
      // Refresh open request tabs from disk; flag conflicts with unsaved edits.
      const tabs = get().tabs.filter((t): t is RequestTab => t.kind === "request" && t.workspaceId === workspaceId);
      await Promise.all(
        tabs.map(async (tab) => {
          let disk: Request;
          try {
            disk = normaliseRequest(await api.getRequest(workspaceId, tab.requestId));
          } catch {
            return; // deleted or unreadable; leave the tab alone
          }
          const diskJson = stableStringify(disk);
          if (diskJson === tab.base) return;
          get().updateTab<RequestTab>(tab.id, (t) =>
            tabDirty(t) ? { ...t, diskVersion: disk } : { ...t, draft: disk, base: diskJson, diskVersion: null },
          );
        }),
      );
    },

    bumpGit: () => set((s) => ({ gitEpoch: s.gitEpoch + 1 })),

    setLayout: (layout) => {
      try {
        localStorage.setItem(LAYOUT_KEY, layout);
      } catch {
        // storage unavailable
      }
      set({ layout });
    },

    openPalette: (mode = "search") => set({ palette: mode }),
    closePalette: () => set({ palette: null }),

    newRequestHere: async () => {
      const id = wsId();
      const tree = get().tree;
      if (!id || !tree) return;
      const tab = get().tabs.find((t) => t.id === get().activeTab[id]);
      let parentId: string | null = null;
      if (tab?.kind === "request") {
        const chain = findChain(tree.collections, tab.requestId);
        parentId = chain?.[chain.length - 1]?.meta.id ?? null;
      } else if (tab?.kind === "group") {
        parentId = tab.groupId;
      }
      if (!parentId) {
        const scoped = get().activeEntry()?.prefs.scopedCollection;
        parentId = scoped ?? tree.collections[0]?.meta.id ?? null;
      }
      try {
        if (!parentId) {
          parentId = (await api.createCollection(id, "Requests")).meta.id;
        }
        get().setView("collections");
        await get().createRequest(parentId);
      } catch (e) {
        fail(e);
      }
    },

    importRequest: async (parentId, request) => {
      const id = wsId();
      if (!id) return;
      try {
        const created = await api.createRequest(id, parentId, request);
        await get().reloadTree();
        get().setView("collections");
        await get().openRequest(created.id);
        get().toast("success", `Imported "${created.name}"`);
      } catch (e) {
        fail(e);
      }
    },

    selectWorkspaceByIndex: async (index) => {
      const w = get().workspaces[index];
      if (w && w.id !== wsId()) await get().selectWorkspace(w.id);
    },

    toast: (kind, message) => {
      const id = uid();
      set((s) => ({ toasts: [...s.toasts.slice(-3), { id, kind, message }] }));
      setTimeout(() => get().dismissToast(id), kind === "error" ? 8000 : 4000);
    },

    dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

    prompt: (opts) =>
      new Promise((resolve) =>
        set({
          dialog: {
            kind: "prompt",
            ...opts,
            resolve: (v) => {
              set({ dialog: null });
              resolve(v);
            },
          },
        }),
      ),

    confirm: (opts) =>
      new Promise((resolve) =>
        set({
          dialog: {
            kind: "confirm",
            ...opts,
            resolve: (v) => {
              set({ dialog: null });
              resolve(v);
            },
          },
        }),
      ),

    showWorkspaceDialog: (mode) => set({ dialog: { kind: "workspace", mode } }),
    showImportDialog: (tab) => set({ dialog: { kind: "import", tab } }),

    closeDialog: () => set({ dialog: null }),
  };
});

/** Tabs of the active workspace. */
export function useWorkspaceTabs(): { tabs: Tab[]; active: Tab | null } {
  const tabs = useStore((s) => s.tabs);
  const wsId = useStore((s) => s.activeWorkspaceId);
  const activeId = useStore((s) => (s.activeWorkspaceId ? s.activeTab[s.activeWorkspaceId] : null));
  const mine = tabs.filter((t) => t.workspaceId === wsId);
  return { tabs: mine, active: mine.find((t) => t.id === activeId) ?? null };
}
