// Typed access to the Rust backend.
//
// Inside the desktop app every call goes through Tauri's `invoke`. In a plain
// browser (`npm run dev` without Tauri) calls go to an in-memory mock so the
// UI can be developed and previewed without building the Rust side.

import type {
  AgentOverview,
  AgentPermissions,
  AppInfo,
  Branch,
  CollectionRun,
  Commit,
  ConflictPreview,
  CookieInfo,
  CookieKey,
  CreatePullRequestInput,
  Environment,
  EnvironmentView,
  ExportRequest,
  FileChanges,
  GroupMeta,
  HistoryEntry,
  GroupNode,
  Identity,
  ImportOptions,
  ImportSummary,
  LocalState,
  MergeSide,
  OAuthToken,
  OAuthTokenStatus,
  MergeOutcome,
  OnboardingSpec,
  PrContext,
  PrSettings,
  PullMode,
  PullRequest,
  Remote,
  RepoStatus,
  Request,
  Resolution,
  Review,
  AddReviewCommentInput,
  ReviewComment,
  RunnerOptions,
  RunResult,
  SavedResponse,
  SigningSettings,
  SparseCheckoutStatus,
  StashEntry,
  SyncReport,
  SyncSettings,
  Tag,
  VaultProviderStatus,
  VaultSettings,
  WorkspaceEntry,
  WorkspaceManifest,
  WorkspacePrefs,
  WorkspaceSummary,
  WorkspaceTemplate,
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
  createWorkspace: (path: string, name: string, team: string | null, initGit: boolean, template: string | null = null) =>
    call<WorkspaceEntry>("create_workspace", { path, name, team, initGit, template }),
  openWorkspace: (path: string) => call<WorkspaceEntry>("open_workspace", { path }),
  cloneWorkspace: (
    url: string,
    path: string,
    branch: string | null,
    subdir: string | null,
    sparsePatterns: string[] | null = null,
  ) => call<WorkspaceEntry>("clone_workspace", { url, path, branch, subdir, sparsePatterns }),
  removeWorkspace: (id: string) => call<void>("remove_workspace", { id }),
  setActiveWorkspace: (id: string) => call<void>("set_active_workspace", { id }),
  updateWorkspacePrefs: (id: string, prefs: WorkspacePrefs) =>
    call<void>("update_workspace_prefs", { id, prefs }),
  updateSyncSettings: (id: string, sync: SyncSettings) =>
    call<void>("update_sync_settings", { id, sync }),
  updateManifest: (id: string, manifest: WorkspaceManifest) =>
    call<void>("update_manifest", { id, manifest }),
  inspectPath: (path: string) => call<"workspace" | "missing" | "directory" | "file">("inspect_path", { path }),

  // templates & onboarding
  listWorkspaceTemplates: () => call<WorkspaceTemplate[]>("list_workspace_templates"),
  /** `courion://onboard?...` link for teammates; repo and branch default to `origin` and HEAD. */
  createOnboardingLink: (workspaceId: string, repoUrl: string | null = null, branch: string | null = null) =>
    call<string>("create_onboarding_link", { workspaceId, repoUrl, branch }),
  parseOnboardingLink: (link: string) => call<OnboardingSpec>("parse_onboarding_link", { link }),
  /** Clone, set up the environment (with the secrets asked for) and register, in one step. */
  onboardWorkspace: (link: string, path: string, secrets: Record<string, string> | null = null) =>
    call<WorkspaceEntry>("onboard_workspace", { link, path, secrets }),

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
  moveItem: (workspaceId: string, itemId: string, targetParentId: string | null, beforeId: string | null) =>
    call<void>("move_item", { workspaceId, itemId, targetParentId, beforeId }),

  // environments & variables
  saveEnvironment: (workspaceId: string, environment: Environment) =>
    call<Environment>("save_environment", { workspaceId, environment }),
  deleteEnvironment: (workspaceId: string, id: string) =>
    call<void>("delete_environment", { workspaceId, id }),
  setEnvironmentOverride: (workspaceId: string, environmentId: string, key: string, value: string | null) =>
    call<void>("set_environment_override", { workspaceId, environmentId, key, value }),
  getLocalState: (workspaceId: string) => call<LocalState>("get_local_state", { workspaceId }),
  clearRuntimeVariables: (workspaceId: string) => call<void>("clear_runtime_variables", { workspaceId }),

  // vault & keychain
  getVaultSettings: (workspaceId: string) => call<VaultSettings>("get_vault_settings", { workspaceId }),
  saveVaultSettings: (workspaceId: string, settings: VaultSettings) =>
    call<void>("save_vault_settings", { workspaceId, settings }),
  vaultProvidersStatus: (workspaceId: string) => call<VaultProviderStatus[]>("vault_providers_status", { workspaceId }),
  resolveSecretReference: (workspaceId: string, value: string) =>
    call<string>("resolve_secret_reference", { workspaceId, value }),

  // runs
  sendRequest: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<RunResult>("send_request", { workspaceId, request, parentId, environmentId }),
  runCollection: (workspaceId: string, groupId: string, environmentId: string | null, options: RunnerOptions) =>
    call<CollectionRun>("run_collection", {
      workspaceId,
      groupId,
      environmentId,
      stopOnFailure: options.stopOnFailure ?? false,
      iterations: options.iterations ?? null,
      delayMs: options.delayMs ?? null,
      data: options.data?.text ?? null,
      dataName: options.data?.name ?? null,
    }),

  // saved responses
  listSavedResponses: (workspaceId: string, requestId: string) =>
    call<SavedResponse[]>("list_saved_responses", { workspaceId, requestId }),
  saveResponse: (workspaceId: string, requestId: string, name: string, run: RunResult) =>
    call<SavedResponse>("save_response", { workspaceId, requestId, name, run }),
  deleteSavedResponse: (workspaceId: string, requestId: string, responseId: string) =>
    call<void>("delete_saved_response", { workspaceId, requestId, responseId }),

  // export and files
  exportRequest: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null, resolve: boolean) =>
    call<ExportRequest>("export_request", { workspaceId, request, parentId, environmentId, resolve }),
  writeFile: (path: string, contents: { text?: string; base64?: string }) =>
    call<void>("write_file", { path, text: contents.text ?? null, base64: contents.base64 ?? null }),

  // OAuth 2.0 tokens (settings from the request's auth or a parent's)
  oauthTokenStatus: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<OAuthTokenStatus>("oauth_token_status", { workspaceId, request, parentId, environmentId }),
  oauthGetToken: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<OAuthToken>("oauth_get_token", { workspaceId, request, parentId, environmentId }),
  oauthCancel: () => call<void>("oauth_cancel"),
  oauthRefreshToken: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<OAuthToken>("oauth_refresh_token", { workspaceId, request, parentId, environmentId }),
  oauthClearToken: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<boolean>("oauth_clear_token", { workspaceId, request, parentId, environmentId }),
  oauthIntrospectToken: (workspaceId: string, request: Request, parentId: string | null, environmentId: string | null) =>
    call<Record<string, unknown>>("oauth_introspect_token", { workspaceId, request, parentId, environmentId }),

  // cookie jar
  listCookies: (workspaceId: string) => call<CookieInfo[]>("list_cookies", { workspaceId }),
  saveCookie: (workspaceId: string, cookie: CookieInfo, previous: CookieKey | null) =>
    call<CookieInfo>("save_cookie", { workspaceId, cookie, previous }),
  deleteCookie: (workspaceId: string, key: CookieKey) => call<boolean>("delete_cookie", { workspaceId, key }),
  clearCookies: (workspaceId: string, domain: string | null) => call<number>("clear_cookies", { workspaceId, domain }),

  // environments: git and protection
  setEnvironmentShared: (workspaceId: string, id: string, shared: boolean) =>
    call<EnvironmentView>("set_environment_shared", { workspaceId, id, shared }),
  importJson: (workspaceId: string, json: string | null, path?: string | null, options?: ImportOptions) =>
    call<ImportSummary>("import_json", { workspaceId, json, path, options: options ?? null }),
  readImportFile: (path: string) => call<string>("read_import_file", { path }),

  // agents
  agentOverview: (workspaceId: string | null) => call<AgentOverview>("agent_overview", { workspaceId }),
  setAgentPermissions: (permissions: AgentPermissions) => call<AgentPermissions>("set_agent_permissions", { permissions }),
  workspaceSummaries: () => call<WorkspaceSummary[]>("workspace_summaries"),

  // history
  getHistory: (workspaceId: string) => call<HistoryEntry[]>("get_history", { workspaceId }),
  clearHistory: (workspaceId: string) => call<void>("clear_history", { workspaceId }),
  restoreHistoryEntry: (workspaceId: string, entryId: string, parentId: string | null) =>
    call<Request>("restore_history_entry", { workspaceId, entryId, parentId }),

  // pull requests & inline review comments
  getPrSettings: (workspaceId: string) => call<PrContext>("get_pr_settings", { workspaceId }),
  savePrSettings: (workspaceId: string, settings: PrSettings) =>
    call<void>("save_pr_settings", { workspaceId, settings }),
  listPullRequests: (workspaceId: string, stateFilter: string | null = null) =>
    call<PullRequest[]>("list_pull_requests", { workspaceId, stateFilter }),
  createPullRequest: (workspaceId: string, input: CreatePullRequestInput) =>
    call<PullRequest>("create_pull_request", { workspaceId, input }),
  approvePullRequest: (workspaceId: string, prNumber: number, body: string | null = null) =>
    call<PullRequest>("approve_pull_request", { workspaceId, prNumber, body }),
  mergePullRequest: (workspaceId: string, prNumber: number, method: string | null = null) =>
    call<PullRequest>("merge_pull_request", { workspaceId, prNumber, method }),
  listReviewComments: (
    workspaceId: string,
    opts: { prNumber?: number | null; requestId?: string | null } = {},
  ) =>
    call<ReviewComment[]>("list_review_comments", {
      workspaceId,
      prNumber: opts.prNumber ?? null,
      requestId: opts.requestId ?? null,
    }),
  addReviewComment: (workspaceId: string, input: AddReviewCommentInput) =>
    call<ReviewComment>("add_review_comment", { workspaceId, input }),
  resolveReviewComment: (workspaceId: string, commentId: string, resolved: boolean) =>
    call<ReviewComment>("resolve_review_comment", { workspaceId, commentId, resolved }),

  // git
  gitFileChanges: (workspaceId: string, path: string, staged: boolean) =>
    call<FileChanges>("git_file_changes", { workspaceId, path, staged }),
  gitConflictPreview: (workspaceId: string, path: string, choices: Record<string, MergeSide> = {}) =>
    call<ConflictPreview>("git_conflict_preview", { workspaceId, path, choices }),
  gitResolveSemantic: (
    workspaceId: string,
    path: string,
    choices: Record<string, MergeSide>,
    manualContent: string | null = null,
  ) => call<void>("git_resolve_semantic", { workspaceId, path, choices, manualContent }),
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
  gitStashes: (workspaceId: string) => call<StashEntry[]>("git_stashes", { workspaceId }),
  gitStashPush: (workspaceId: string, message: string | null = null, includeUntracked = true) =>
    call<void>("git_stash_push", { workspaceId, message, includeUntracked }),
  gitStashApply: (workspaceId: string, index: number, pop: boolean) =>
    call<MergeOutcome>("git_stash_apply", { workspaceId, index, pop }),
  gitStashDrop: (workspaceId: string, index: number) =>
    call<void>("git_stash_drop", { workspaceId, index }),
  gitCherryPick: (workspaceId: string, commit: string) =>
    call<MergeOutcome>("git_cherry_pick", { workspaceId, commit }),
  gitRevert: (workspaceId: string, commit: string) =>
    call<MergeOutcome>("git_revert", { workspaceId, commit }),
  gitTags: (workspaceId: string) => call<Tag[]>("git_tags", { workspaceId }),
  gitCreateTag: (workspaceId: string, name: string, target: string | null = null, message: string | null = null) =>
    call<void>("git_create_tag", { workspaceId, name, target, message }),
  gitDeleteTag: (workspaceId: string, name: string) =>
    call<void>("git_delete_tag", { workspaceId, name }),
  gitPushTag: (workspaceId: string, name: string, remote: string | null = null) =>
    call<void>("git_push_tag", { workspaceId, remote, name }),
  gitSparseCheckoutStatus: (workspaceId: string) =>
    call<SparseCheckoutStatus>("git_sparse_checkout_status", { workspaceId }),
  gitSparseCheckoutSet: (workspaceId: string, patterns: string[], cone = true) =>
    call<void>("git_sparse_checkout_set", { workspaceId, patterns, cone }),
  gitSparseCheckoutDisable: (workspaceId: string) =>
    call<void>("git_sparse_checkout_disable", { workspaceId }),
  gitSigningSettings: (workspaceId: string) =>
    call<SigningSettings>("git_signing_settings", { workspaceId }),
  gitSetSigningSettings: (workspaceId: string, settings: SigningSettings) =>
    call<void>("git_set_signing_settings", { workspaceId, settings }),
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

/** Native file picker; `null` when cancelled or unavailable. */
export async function pickFile(title: string, extensions: string[]): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, title, filters: [{ name: "Specs and exports", extensions }] });
  return typeof picked === "string" ? picked : null;
}

/**
 * Save text or binary content to a file the user picks. In the browser
 * preview it downloads instead. Returns false when the user cancels.
 */
export async function saveToFile(name: string, contents: { text?: string; base64?: string }, mime = "application/octet-stream"): Promise<boolean> {
  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: name, title: "Save response body" });
    if (!path) return false;
    await api.writeFile(path, contents);
    return true;
  }
  const blob = contents.base64
    ? new Blob([Uint8Array.from(atob(contents.base64), (c) => c.charCodeAt(0))], { type: mime })
    : new Blob([contents.text ?? ""], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return true;
}

/** Window controls for the custom title bar (no-ops in the browser preview). */
export async function windowAction(action: "minimize" | "toggleMaximize" | "close"): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const w = getCurrentWindow();
  if (action === "minimize") await w.minimize();
  else if (action === "toggleMaximize") await w.toggleMaximize();
  else await w.close();
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
