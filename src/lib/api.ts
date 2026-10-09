// Typed access to the Rust backend.
//
// Inside the desktop app every call goes through Tauri's `invoke`. In a plain
// browser (`npm run dev` without Tauri) calls go to an in-memory mock so the
// UI can be developed and previewed without building the Rust side.

import type {
  AppInfo,
  Branch,
  CollectionRun,
  Commit,
  Environment,
  GroupMeta,
  GroupNode,
  Identity,
  LocalState,
  MergeOutcome,
  PullMode,
  Remote,
  RepoStatus,
  Request,
  Resolution,
  Review,
  RunResult,
  SavedResponse,
  SyncReport,
  SyncSettings,
  WorkspaceEntry,
  WorkspaceManifest,
  WorkspacePrefs,
  WorkspaceTree,
} from "./types";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  const { mockInvoke } = await import("./mock");
  return mockInvoke<T>(cmd, args);
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),

  // registry
  listWorkspaces: () =>
    call<{ workspaces: WorkspaceEntry[]; activeWorkspace: string | null }>("list_workspaces"),
  createWorkspace: (path: string, name: string, team: string | null, initGit: boolean) =>
    call<WorkspaceEntry>("create_workspace", { path, name, team, initGit }),
  openWorkspace: (path: string) => call<WorkspaceEntry>("open_workspace", { path }),
  cloneWorkspace: (url: string, path: string, branch: string | null, subdir: string | null) =>
    call<WorkspaceEntry>("clone_workspace", { url, path, branch, subdir }),
  removeWorkspace: (id: string) => call<void>("remove_workspace", { id }),
  setActiveWorkspace: (id: string) => call<void>("set_active_workspace", { id }),
  updateWorkspacePrefs: (id: string, prefs: WorkspacePrefs) =>
    call<void>("update_workspace_prefs", { id, prefs }),
  updateSyncSettings: (id: string, sync: SyncSettings) =>
    call<void>("update_sync_settings", { id, sync }),
  updateManifest: (id: string, manifest: WorkspaceManifest) =>
    call<void>("update_manifest", { id, manifest }),
  inspectPath: (path: string) => call<"workspace" | "missing" | "directory" | "file">("inspect_path", { path }),

  // contents
  loadWorkspace: (id: string) => call<WorkspaceTree>("load_workspace", { id }),
  createCollection: (workspaceId: string, name: string) =>
    call<GroupNode>("create_collection", { workspaceId, name }),
  createFolder: (workspaceId: string, parentId: string, name: string) =>
    call<GroupNode>("create_folder", { workspaceId, parentId, name }),
  getGroup: (workspaceId: string, id: string) => call<GroupMeta>("get_group", { workspaceId, id }),
  updateGroup: (workspaceId: string, meta: GroupMeta) => call<void>("update_group", { workspaceId, meta }),
  deleteGroup: (workspaceId: string, id: string) => call<void>("delete_group", { workspaceId, id }),
  getRequest: (workspaceId: string, id: string) => call<Request>("get_request", { workspaceId, id }),
  createRequest: (workspaceId: string, parentId: string, request: Request) =>
    call<Request>("create_request", { workspaceId, parentId, request }),
  saveRequest: (workspaceId: string, request: Request) =>
    call<void>("save_request", { workspaceId, request }),
  deleteRequest: (workspaceId: string, id: string) => call<void>("delete_request", { workspaceId, id }),
  duplicateRequest: (workspaceId: string, id: string) =>
    call<Request>("duplicate_request", { workspaceId, id }),
  moveRequest: (workspaceId: string, id: string, parentId: string) =>
    call<void>("move_request", { workspaceId, id, parentId }),

  // environments & variables
  saveEnvironment: (workspaceId: string, environment: Environment) =>
    call<Environment>("save_environment", { workspaceId, environment }),
  deleteEnvironment: (workspaceId: string, id: string) =>
    call<void>("delete_environment", { workspaceId, id }),
  setEnvironmentOverride: (workspaceId: string, environmentId: string, key: string, value: string | null) =>
    call<void>("set_environment_override", { workspaceId, environmentId, key, value }),
  getLocalState: (workspaceId: string) => call<LocalState>("get_local_state", { workspaceId }),
  clearRuntimeVariables: (workspaceId: string) => call<void>("clear_runtime_variables", { workspaceId }),

  // runs
  sendRequest: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<RunResult>("send_request", { workspaceId, request, parentId, environmentId }),
  runCollection: (workspaceId: string, groupId: string, environmentId: string | null, stopOnFailure: boolean) =>
    call<CollectionRun>("run_collection", { workspaceId, groupId, environmentId, stopOnFailure }),

  // saved responses
  listSavedResponses: (workspaceId: string, requestId: string) =>
    call<SavedResponse[]>("list_saved_responses", { workspaceId, requestId }),
  saveResponse: (workspaceId: string, requestId: string, name: string, run: RunResult) =>
    call<SavedResponse>("save_response", { workspaceId, requestId, name, run }),
  deleteSavedResponse: (workspaceId: string, requestId: string, responseId: string) =>
    call<void>("delete_saved_response", { workspaceId, requestId, responseId }),

  // git
  gitStatus: (workspaceId: string) => call<RepoStatus>("git_status", { workspaceId }),
  gitInit: (workspaceId: string) => call<void>("git_init", { workspaceId }),
  gitStage: (workspaceId: string, paths: string[]) => call<void>("git_stage", { workspaceId, paths }),
  gitUnstage: (workspaceId: string, paths: string[]) => call<void>("git_unstage", { workspaceId, paths }),
  gitDiscard: (workspaceId: string, paths: string[]) => call<void>("git_discard", { workspaceId, paths }),
  gitCommit: (workspaceId: string, message: string, amend = false) =>
    call<Commit>("git_commit", { workspaceId, message, amend }),
  gitFetch: (workspaceId: string) => call<void>("git_fetch", { workspaceId }),
  gitPull: (workspaceId: string, mode: PullMode) => call<MergeOutcome>("git_pull", { workspaceId, mode }),
  gitPush: (workspaceId: string, forceWithLease = false) =>
    call<void>("git_push", { workspaceId, forceWithLease }),
  gitBranches: (workspaceId: string) => call<Branch[]>("git_branches", { workspaceId }),
  gitCreateBranch: (workspaceId: string, name: string, start: string | null, checkout: boolean) =>
    call<void>("git_create_branch", { workspaceId, name, start, checkout }),
  gitCheckout: (workspaceId: string, name: string) => call<void>("git_checkout", { workspaceId, name }),
  gitDeleteBranch: (workspaceId: string, name: string, force: boolean) =>
    call<void>("git_delete_branch", { workspaceId, name, force }),
  gitMerge: (workspaceId: string, branch: string, noFf: boolean) =>
    call<MergeOutcome>("git_merge", { workspaceId, branch, noFf }),
  gitAbort: (workspaceId: string) => call<void>("git_abort", { workspaceId }),
  gitContinue: (workspaceId: string) => call<void>("git_continue", { workspaceId }),
  gitResolve: (workspaceId: string, path: string, resolution: Resolution) =>
    call<void>("git_resolve", { workspaceId, path, resolution }),
  gitLog: (workspaceId: string, limit = 50, rev: string | null = null, path: string | null = null) =>
    call<Commit[]>("git_log", { workspaceId, rev, limit, path }),
  gitDiff: (workspaceId: string, path: string | null, staged: boolean) =>
    call<string>("git_diff", { workspaceId, path, staged }),
  gitShow: (workspaceId: string, commit: string) => call<string>("git_show", { workspaceId, commit }),
  gitReview: (workspaceId: string, base: string, head: string) =>
    call<Review>("git_review", { workspaceId, base, head }),
  gitCompareUrl: (workspaceId: string, base: string, head: string) =>
    call<string | null>("git_compare_url", { workspaceId, base, head }),
  gitRemotes: (workspaceId: string) => call<Remote[]>("git_remotes", { workspaceId }),
  gitAddRemote: (workspaceId: string, name: string, url: string) =>
    call<void>("git_add_remote", { workspaceId, name, url }),
  gitSetRemoteUrl: (workspaceId: string, name: string, url: string) =>
    call<void>("git_set_remote_url", { workspaceId, name, url }),
  gitRemoveRemote: (workspaceId: string, name: string) =>
    call<void>("git_remove_remote", { workspaceId, name }),
  gitIdentity: (workspaceId: string) => call<Identity>("git_identity", { workspaceId }),
  gitSetIdentity: (workspaceId: string, name: string, email: string) =>
    call<void>("git_set_identity", { workspaceId, name, email }),
  syncNow: (workspaceId: string) => call<SyncReport | null>("sync_now", { workspaceId }),
  syncStatuses: () => call<Record<string, SyncReport>>("sync_statuses"),
};

/** Subscribe to a backend event. Returns an unsubscribe function. */
export async function onEvent<T>(name: string, handler: (payload: T) => void): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(name, (e) => handler(e.payload));
  }
  const { mockListen } = await import("./mock");
  return mockListen(name, handler as (p: unknown) => void);
}

/** Native folder picker; `null` when cancelled or unavailable. */
export async function pickDirectory(title: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

export async function openExternal(url: string): Promise<void> {
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
