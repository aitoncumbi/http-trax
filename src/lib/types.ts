// TypeScript mirrors of the Rust types (crates/trax-core/src/model.rs,
// crates/trax-git). Keep in sync when the backend model changes.

export interface KeyValue {
  key: string;
  value: string;
  enabled: boolean;
  description?: string;
}

export interface Variable {
  key: string;
  value: string;
  enabled: boolean;
  secret?: boolean;
}

export interface MultipartField {
  key: string;
  value: string;
  isFile?: boolean;
  enabled: boolean;
}

export type Body =
  | { mode: "none" }
  | { mode: "json"; text: string }
  | { mode: "text"; text: string; contentType?: string }
  | { mode: "xml"; text: string }
  | { mode: "form-urlencoded"; fields: KeyValue[] }
  | { mode: "multipart"; fields: MultipartField[] }
  | { mode: "graphql"; query: string; variables: string };

export type BodyMode = Body["mode"];

export type Auth =
  | { type: "inherit" }
  | { type: "none" }
  | { type: "bearer"; token: string }
  | { type: "basic"; username: string; password: string }
  | { type: "api-key"; key: string; value: string; location: "header" | "query" };

export type AuthType = Auth["type"];

export interface Scripts {
  preRequest?: string;
  postResponse?: string;
}

export interface RequestSettings {
  timeoutMs?: number;
  followRedirects: boolean;
  verifyTls: boolean;
}

export interface Request {
  id: string;
  name: string;
  seq: number;
  description?: string;
  method: string;
  url: string;
  params?: KeyValue[];
  headers?: KeyValue[];
  body?: Body;
  auth?: Auth;
  scripts?: Scripts;
  settings?: RequestSettings;
}

export interface GroupMeta {
  id: string;
  name: string;
  seq: number;
  description?: string;
  variables?: Variable[];
  headers?: KeyValue[];
  auth?: Auth;
  scripts?: Scripts;
}

export interface Environment {
  id: string;
  name: string;
  variables: Variable[];
}

export interface EnvironmentView extends Environment {
  /** Local "current values" (set by scripts or edited locally). */
  overrides: Record<string, string>;
}

export interface RequestSummary {
  id: string;
  name: string;
  method: string;
  url: string;
  seq: number;
  path: string;
}

export interface GroupNode {
  meta: GroupMeta;
  dir: string;
  items: TreeItem[];
}

export type TreeItem = ({ kind: "folder" } & GroupNode) | ({ kind: "request" } & RequestSummary);

export interface WorkspaceManifest {
  version: number;
  id: string;
  name: string;
  description?: string;
  team?: string;
  variables?: Variable[];
}

export interface LoadProblem {
  path: string;
  message: string;
}

export interface WorkspaceTree {
  manifest: WorkspaceManifest;
  root: string;
  collections: GroupNode[];
  environments: EnvironmentView[];
  problems: LoadProblem[];
}

export interface SyncSettings {
  enabled: boolean;
  intervalSecs: number;
  autoPull: boolean;
  remote: string;
}

export interface WorkspacePrefs {
  bookmarks: string[];
  scopedCollection?: string | null;
  activeEnvironment?: string | null;
}

export interface WorkspaceEntry {
  id: string;
  name: string;
  path: string;
  team?: string | null;
  sync: SyncSettings;
  prefs: WorkspacePrefs;
}

export interface Header {
  key: string;
  value: string;
}

export interface SentRequest {
  method: string;
  url: string;
  headers: Header[];
  body?: string;
}

export interface ResponseData {
  status: number;
  statusText: string;
  httpVersion: string;
  headers: Header[];
  body: string;
  bodyBase64?: string;
  size: number;
  timeMs: number;
  url: string;
}

export interface TestResult {
  name: string;
  passed: boolean;
  skipped?: boolean;
  error?: string;
  source: string;
}

export interface LogEntry {
  level: string;
  message: string;
  phase: string;
  source: string;
}

export interface ScriptError {
  phase: string;
  source: string;
  message: string;
}

export interface RunResult {
  id: string;
  requestId?: string | null;
  requestName: string;
  startedAt: string;
  request: SentRequest;
  response?: ResponseData | null;
  error?: string | null;
  tests: TestResult[];
  logs: LogEntry[];
  scriptErrors: ScriptError[];
  skipped: boolean;
}

export interface SavedResponse {
  id: string;
  requestId: string;
  name: string;
  savedAt: string;
  request: SentRequest;
  response: ResponseData;
}

export interface CollectionRunItem {
  requestId: string;
  name: string;
  method: string;
  url: string;
  status?: number | null;
  timeMs?: number | null;
  passed: number;
  failed: number;
  error?: string | null;
  ok: boolean;
}

export interface CollectionRunReport {
  groupId: string;
  groupName: string;
  environment?: string | null;
  items: CollectionRunItem[];
  total: number;
  succeeded: number;
  failed: number;
  testsPassed: number;
  testsFailed: number;
  durationMs: number;
}

export interface CollectionRun {
  report: CollectionRunReport;
  results: RunResult[];
}

export interface LocalState {
  runtimeVariables: Record<string, string>;
  overrides: Record<string, Record<string, string>>;
}

export interface AppInfo {
  version: string;
  gitVersion?: string | null;
  mcpCommand?: string | null;
  configDir: string;
  platform: string;
}

// ------------------------------------------------------------------- git

export interface FileStatus {
  path: string;
  origPath?: string | null;
  index: string;
  worktree: string;
  staged: boolean;
  unstaged: boolean;
  untracked: boolean;
  conflicted: boolean;
  inWorkspace: boolean;
}

export interface RepoStatus {
  isRepo: boolean;
  root: string;
  prefix: string;
  branch?: string | null;
  detached: boolean;
  head?: string | null;
  upstream?: string | null;
  ahead: number;
  behind: number;
  files: FileStatus[];
  merging: boolean;
  rebasing: boolean;
  conflicts: number;
  hasRemote: boolean;
}

export interface Branch {
  name: string;
  remote: boolean;
  current: boolean;
  upstream?: string | null;
  ahead: number;
  behind: number;
  lastCommit: string;
  lastCommitDate?: number | null;
}

export interface Commit {
  hash: string;
  shortHash: string;
  authorName: string;
  authorEmail: string;
  date: number;
  subject: string;
  parents: string[];
  refs: string[];
}

export interface Remote {
  name: string;
  url: string;
  webUrl?: string | null;
}

export type PullMode = "ff-only" | "merge" | "rebase";

export type MergeOutcome =
  | { kind: "up-to-date" }
  | { kind: "fast-forward"; commits: number }
  | { kind: "merged" }
  | { kind: "rebased" }
  | { kind: "conflicts"; files: string[] };

export type Resolution = "ours" | "theirs" | "resolved";

export interface ChangedFile {
  path: string;
  origPath?: string | null;
  status: string;
  additions?: number | null;
  deletions?: number | null;
}

export interface Review {
  base: string;
  head: string;
  mergeBase?: string | null;
  ahead: number;
  behind: number;
  commits: Commit[];
  files: ChangedFile[];
  diff: string;
  conflicts?: string[] | null;
}

export interface Identity {
  name?: string | null;
  email?: string | null;
}

export type SyncState =
  | "synced"
  | "behind"
  | "ahead"
  | "diverged"
  | "conflicts"
  | "no-upstream"
  | "local-only"
  | "not-repo"
  | "offline"
  | "auth-required"
  | "error";

export interface SyncReport {
  state: SyncState;
  branch?: string | null;
  upstream?: string | null;
  ahead: number;
  behind: number;
  changes: number;
  pulled: number;
  fetchedAt?: string | null;
  checkedAt: string;
  message?: string | null;
}
