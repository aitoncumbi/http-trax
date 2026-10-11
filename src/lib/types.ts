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
  | { type: "digest"; username: string; password: string }
  | {
      type: "aws-sigv4";
      accessKey: string;
      secretKey: string;
      region: string;
      service: string;
      sessionToken?: string;
    }
  | { type: "ntlm"; username: string; password: string; domain?: string }
  | { type: "api-key"; key: string; value: string; location: "header" | "query" }
  | ({ type: "oauth2" } & OAuth2);

export type AuthType = Auth["type"];

export type OAuth2Grant = "client-credentials" | "authorization-code" | "device-code" | "password";

/** OAuth 2.0 settings; text fields take {{variables}}. */
export interface OAuth2 {
  grant: OAuth2Grant;
  authUrl?: string;
  deviceAuthUrl?: string;
  tokenUrl: string;
  clientId: string;
  clientSecret?: string;
  scope?: string;
  /** Loopback redirect; empty means a random port on 127.0.0.1. */
  redirectUri?: string;
  /** Default true. */
  pkce?: boolean;
  username?: string;
  password?: string;
  clientAuth?: "basic" | "body" | "private-key-jwt";
  privateKeyPem?: string;
  keyId?: string;
  introspectionUrl?: string;
  extraParams?: KeyValue[];
  /** Default "Bearer". */
  headerPrefix?: string;
}

export interface OAuthToken {
  accessToken: string;
  tokenType: string;
  refreshToken?: string | null;
  expiresAt?: string | null;
  scope?: string | null;
  idToken?: string | null;
  obtainedAt: string;
}

export interface OAuthTokenStatus {
  token: OAuthToken | null;
  /** Valid for at least another 30 seconds. */
  fresh: boolean;
  /** Authorization code and device code need the user to sign in. */
  needsUser: boolean;
}

/** What the user has to do while a token is being fetched. */
export type OAuthPrompt =
  | { kind: "browser"; url: string }
  | { kind: "device"; userCode: string; verificationUri: string; verificationUriComplete?: string | null; expiresIn: number };

export interface Scripts {
  preRequest?: string;
  postResponse?: string;
}

export interface RequestSettings {
  timeoutMs?: number;
  followRedirects: boolean;
  verifyTls: boolean;
  /** Use the workspace cookie jar (default true). */
  cookies?: boolean;
  proxyUrl?: string;
  caCert?: string;
  clientCert?: string;
  clientKey?: string;
}

/**
 * What responses must look like. The nearest setting applies: request, then
 * folders, then the collection. Paths are relative to the workspace root.
 */
export interface Contract {
  /** OpenAPI 3.x or Swagger 2.0 document (YAML or JSON). */
  openapi?: string;
  /** JSON Schema the response body must match. */
  schema?: string;
  /** No checks here, whatever folders or the collection set. */
  off?: boolean;
}

/** A cookie in the workspace jar (`.trax/cookies.json`, never committed). */
export interface CookieInfo {
  name: string;
  value: string;
  /** Host, without a leading dot. */
  domain: string;
  /** Also sent to subdomains. */
  includeSubdomains: boolean;
  path: string;
  /** RFC 3339; absent for a session cookie. */
  expires?: string | null;
  secure: boolean;
  httpOnly: boolean;
  sameSite?: string | null;
}

export interface CookieKey {
  domain: string;
  path: string;
  name: string;
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
  contract?: Contract;
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
  contract?: Contract;
}

export interface Environment {
  id: string;
  name: string;
  /** Sending needs confirmation; agents can't run requests with it. */
  protected?: boolean;
  variables: Variable[];
}

export interface EnvironmentView extends Environment {
  /** Local "current values" (set by scripts or edited locally). */
  overrides: Record<string, string>;
  /** Committed to git (environments/) rather than kept on this machine. */
  shared: boolean;
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

/** `git log %G?` verdict; absent when the commit is unsigned. */
export type SignatureStatus = "good" | "untrusted" | "bad" | "expired" | "revoked" | "cannot-check";

export interface Commit {
  hash: string;
  shortHash: string;
  authorName: string;
  authorEmail: string;
  date: number;
  subject: string;
  parents: string[];
  refs: string[];
  signatureStatus?: SignatureStatus;
  signer?: string;
  signingKey?: string;
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
  changedFiles?: string[];
  changedRequests?: string[];
  authors?: string[];
  fetchedAt?: string | null;
  checkedAt: string;
  message?: string | null;
}

export interface HistoryEntry {
  id: string;
  requestId?: string | null;
  name: string;
  method: string;
  url: string;
  status?: number | null;
  timeMs?: number | null;
  ok: boolean;
  error?: string | null;
  testsPassed: number;
  testsFailed: number;
  environment?: string | null;
  request?: SentRequest | null;
  at: string;
}

export type ChangeKind = "added" | "updated" | "removed";

export interface SemanticChange {
  kind: ChangeKind;
  label: string;
  subject: string;
  area: string;
  before?: string | null;
  after?: string | null;
}

export type FileKind = "request" | "environment" | "collection" | "folder" | "workspace" | "saved-response" | "other";

export interface FileChanges {
  path: string;
  kind: FileKind;
  changes?: SemanticChange[] | null;
  diff: string;
  additions: number;
  deletions: number;
}

// ------------------------------------------------------------- agents

export interface AgentPermissions {
  editRequests: boolean;
  runRequests: boolean;
  readResponses: boolean;
  editEnvironments: boolean;
  readSecrets: boolean;
}

export interface AgentSession {
  pid: number;
  client: string;
  clientId: string;
  version?: string | null;
  startedAt: string;
  lastSeen: string;
}

export type AgentClientStatus = "connected" | "configured" | "not-set-up";

export interface AgentClient {
  id: "code" | "desktop" | "vscode" | "other";
  name: string;
  status: AgentClientStatus;
  configPath?: string | null;
  session?: AgentSession | null;
}

export type ActivityKind = "created" | "edited" | "deleted" | "ran";

export interface AgentActivity {
  kind: ActivityKind;
  what: string;
  client: string;
  at: string;
}

export interface AgentOverview {
  permissions: AgentPermissions;
  clients: AgentClient[];
  sessions: AgentSession[];
  activity: AgentActivity[];
}

export interface WorkspaceSummary {
  id: string;
  exists: boolean;
  requests: number;
  collections: number;
  isRepo: boolean;
  remote?: string | null;
  members?: number | null;
}

export interface ImportSummary {
  collections: string[];
  environments: string[];
  requests: number;
  warnings: string[];
}

// ------------------------------------------------------------- export

export interface ExportPart {
  name: string;
  value: string;
  isFile: boolean;
}

export type ExportBody =
  | { kind: "none" }
  | { kind: "text"; text: string }
  | { kind: "form"; fields: [string, string][] }
  | { kind: "multipart"; parts: ExportPart[] };

/** A request as it would be sent (see `export_request`). */
export interface ExportRequest {
  method: string;
  url: string;
  headers: { key: string; value: string }[];
  body: ExportBody;
  basicAuth?: { username: string; password: string } | null;
  warnings: string[];
}

// ------------------------------------------ Phase 2: git & semantic merge

export interface StashEntry {
  index: number;
  reference: string;
  message: string;
  date: number;
}

export interface Tag {
  name: string;
  hash: string;
  shortHash: string;
  /** Annotation message; absent for lightweight tags. */
  message?: string | null;
  date?: number | null;
}

export type SigningFormat = "gpg" | "ssh" | "x509";

export interface SigningSettings {
  enabled: boolean;
  format: SigningFormat;
  signingKey?: string | null;
  program?: string | null;
}

export interface SparseCheckoutStatus {
  enabled: boolean;
  cone: boolean;
  patterns: string[];
}

export interface ConflictStages {
  base?: string | null;
  ours?: string | null;
  theirs?: string | null;
  worktree?: string | null;
}

export type MergeSide = "base" | "ours" | "theirs";

export interface FieldConflict {
  /** Stable id used as the key in the `choices` map. */
  id: string;
  section: string;
  field: string;
  label: string;
  base?: string | null;
  ours?: string | null;
  theirs?: string | null;
  chosen: MergeSide;
}

export interface SemanticMergePreview {
  path: string;
  kind: FileKind;
  canAutoMerge: boolean;
  autoMerged: string[];
  conflicts: FieldConflict[];
  mergedYaml: string;
}

export interface ConflictPreview {
  stages: ConflictStages;
  semantic?: SemanticMergePreview | null;
}

// ------------------------------------- Phase 2: templates & onboarding

export interface WorkspaceTemplate {
  id: string;
  name: string;
  description: string;
  tags: string[];
  environments: string[];
  requestCount: number;
}

export interface OnboardingSpec {
  name: string;
  repoUrl?: string;
  branch?: string;
  defaultEnv?: string;
  templateId?: string;
  sparsePatterns?: string[];
  /** Secret variable names the newcomer is asked for. */
  requiredSecrets?: string[];
  envVars?: Record<string, string>;
}

// ----------------------------------------- Phase 2: vault & keychain

export interface VaultSettings {
  useOsKeychain: boolean;
  hashicorpAddr?: string;
  hashicorpNamespace?: string;
  hashicorpToken?: string;
  onepasswordConnectUrl?: string;
  onepasswordToken?: string;
  azureVaultName?: string;
  azureToken?: string;
}

export interface VaultProviderStatus {
  id: string;
  name: string;
  scheme: string;
  available: boolean;
  detail: string;
}

// ------------------------------ Phase 2: pull requests & inline reviews

export type HostingProvider = "github" | "gitlab";

export interface PrSettings {
  providerOverride?: HostingProvider;
  apiBaseUrl?: string;
  githubToken?: string;
  gitlabToken?: string;
}

export interface HostingRepo {
  provider: HostingProvider;
  host: string;
  apiBase: string;
  webBase: string;
  owner: string;
  repo: string;
  fullPath: string;
}

export interface PrContext {
  settings: PrSettings;
  /** Detected from the `origin` remote; null when it isn't GitHub or GitLab. */
  hosting?: HostingRepo | null;
}

export type PrState = "open" | "merged" | "closed";

export interface PullRequest {
  number: number;
  title: string;
  body: string;
  state: PrState;
  draft: boolean;
  author: string;
  sourceBranch: string;
  targetBranch: string;
  webUrl: string;
  createdAt: string;
  updatedAt: string;
  approved: boolean;
  commentCount: number;
  provider: HostingProvider;
}

export interface CreatePullRequestInput {
  title: string;
  body: string;
  sourceBranch: string;
  targetBranch: string;
  draft: boolean;
}

export interface ReviewComment {
  id: string;
  prNumber?: number;
  requestId?: string;
  requestName?: string;
  path: string;
  /** Request field the comment is about: `url`, `headers.Authorization`, `scripts.postResponse`, `general`… */
  field: string;
  line: number;
  body: string;
  author: string;
  createdAt: string;
  resolved: boolean;
  /** `local`, `github` or `gitlab`. */
  source: string;
}

export interface AddReviewCommentInput {
  prNumber?: number | null;
  requestId?: string | null;
  path?: string | null;
  field?: string | null;
  line?: number | null;
  body: string;
}
