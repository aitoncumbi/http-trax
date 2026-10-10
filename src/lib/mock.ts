// In-memory stand-in for the Rust backend, used when the UI runs in a plain
// browser (`npm run dev`). It exists so the interface can be developed and
// previewed without Tauri; it doesn't send real HTTP requests or run git.

import type {
  AgentActivity,
  AgentPermissions,
  Branch,
  Commit,
  ConflictPreview,
  CookieInfo,
  CookieKey,
  CreatePullRequestInput,
  Environment,
  EnvironmentView,
  ExportRequest,
  OAuth2,
  OAuthToken,
  GroupMeta,
  FileChanges,
  GroupNode,
  HistoryEntry,
  MergeOutcome,
  MergeSide,
  OnboardingSpec,
  PrContext,
  PrSettings,
  PullRequest,
  RepoStatus,
  Request,
  AddReviewCommentInput,
  ReviewComment,
  RunResult,
  SavedResponse,
  SigningSettings,
  SparseCheckoutStatus,
  StashEntry,
  SyncReport,
  Tag,
  TreeItem,
  VaultProviderStatus,
  VaultSettings,
  WorkspaceEntry,
  WorkspaceManifest,
  WorkspaceTemplate,
} from "./types";

type MockItem = { kind: "folder"; group: MockGroup } | { kind: "request"; request: Request };
interface MockGroup {
  meta: GroupMeta;
  items: MockItem[];
}
interface MockWorkspace {
  entry: WorkspaceEntry;
  manifest: WorkspaceManifest;
  collections: MockGroup[];
  environments: EnvironmentView[];
  saved: Record<string, SavedResponse[]>;
  runtime: Record<string, string>;
  history: HistoryEntry[];
}

const uid = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2) + Date.now().toString(36);

// ---------------------------------------------------------------- cookie jar

const jars = new Map<string, CookieInfo[]>();
const jar = (workspaceId: string) => {
  let j = jars.get(workspaceId);
  if (!j) jars.set(workspaceId, (j = []));
  return j;
};
const sameCookie = (c: CookieKey, k: CookieKey) => c.domain === k.domain && c.path === k.path && c.name === k.name;

const kv = (key: string, value: string) => ({ key, value, enabled: true });

function req(name: string, method: string, url: string, extra: Partial<Request> = {}): Request {
  return {
    id: uid(),
    name,
    seq: 0,
    method,
    url,
    settings: { followRedirects: true, verifyTls: true },
    ...extra,
  };
}

function group(name: string, items: MockItem[], extra: Partial<GroupMeta> = {}): MockGroup {
  return { meta: { id: uid(), name, seq: 0, ...extra }, items };
}

const r = (request: Request): MockItem => ({ kind: "request", request });
const f = (g: MockGroup): MockItem => ({ kind: "folder", group: g });

function seed(): MockWorkspace[] {
  const ordersFolder = group("Orders", [
    r(
      req("List orders", "GET", "{{baseUrl}}/v1/orders", {
        description:
          "Returns a page of orders, newest first. Filter with `status` and page with `limit`. The post-response script saves the first order id as `orderId` for the next request.",
        params: [
          { key: "status", value: "pending", enabled: true, description: "Filter by status" },
          { key: "limit", value: "20", enabled: true, description: "Page size" },
          { key: "expand", value: "customer", enabled: false, description: "Embed customer object" },
        ],
        headers: [kv("Accept", "application/json"), { key: "X-Request-Id", value: "{{$uuid}}", enabled: true, description: "New value per send" }],
        scripts: {
          postResponse:
            'trax.test("returns orders", () => {\n  trax.expect(trax.response.status).toBe(200);\n});\n\n// Chain the next request\ntrax.env.set("orderId", trax.response.json().data[0].id);\n',
        },
      }),
    ),
    r(
      req("Create order", "POST", "{{baseUrl}}/v1/orders", {
        headers: [kv("Idempotency-Key", "{{$uuid}}")],
        body: {
          mode: "json",
          text: '{\n  "customerId": "{{customerId}}",\n  "items": [\n    { "sku": "TRX-001", "quantity": 2 }\n  ],\n  "currency": "EUR"\n}',
        },
        scripts: {
          postResponse:
            'trax.test("status is 201", () => {\n  trax.expect(trax.response.status).toBe(201);\n});\n\ntrax.env.set("orderId", trax.response.json().id);\n',
        },
      }),
    ),
    r(req("Get order", "GET", "{{baseUrl}}/v1/orders/{{orderId}}")),
    r(req("Order receipt", "GET", "{{baseUrl}}/v1/orders/{{orderId}}/receipt", { headers: [kv("Accept", "text/html")] })),
    r(req("Cancel order", "DELETE", "{{baseUrl}}/v1/orders/{{orderId}}")),
  ]);
  const admin = group("Admin", [
    r(req("Refund order", "POST", "{{baseUrl}}/v1/orders/{{orderId}}/refund", { body: { mode: "json", text: '{\n  "amount": 2400,\n  "reason": "duplicate"\n}' } })),
    r(req("Export orders", "GET", "{{baseUrl}}/v1/admin/orders/export")),
    r(req("Purge test data", "DELETE", "{{baseUrl}}/v1/admin/test-data")),
  ]);
  const orders = group("Orders API", [f(ordersFolder), f(admin)], {
    variables: [{ key: "apiVersion", value: "v1", enabled: true }],
    auth: { type: "bearer", token: "{{apiToken}}" },
  });
  const customers = group(
    "Customers API",
    [
      r(req("Search customers", "GET", "{{baseUrl}}/v1/customers", { params: [kv("q", "ada")] })),
      r(req("Create customer", "POST", "{{baseUrl}}/v1/customers", { body: { mode: "json", text: '{\n  "name": "Ada Lovelace",\n  "email": "ada@example.com"\n}' } })),
    ],
    {
      auth: {
        type: "oauth2",
        grant: "authorization-code",
        authUrl: "https://id.acme.dev/oauth2/authorize",
        tokenUrl: "https://id.acme.dev/oauth2/token",
        clientId: "courion-desktop",
        scope: "openid customers:read customers:write",
        extraParams: [{ key: "audience", value: "https://api.acme.dev", enabled: true }],
      },
    },
  );
  const auth = group("Auth", [
    r(
      req("Issue access token", "POST", "{{authUrl}}/oauth/token", {
        body: { mode: "form-urlencoded", fields: [kv("grant_type", "client_credentials"), kv("client_id", "{{clientId}}"), kv("client_secret", "{{clientSecret}}")] },
        scripts: { postResponse: 'trax.env.set("apiToken", trax.response.json().access_token);\n' },
      }),
    ),
  ]);
  const webhooks = group("Webhooks", [r(req("Stripe webhook", "POST", "{{baseUrl}}/webhooks/stripe"))]);
  [orders, customers, auth, webhooks].forEach((c, i) => (c.meta.seq = i + 1));

  const env = (
    name: string,
    vars: [string, string, boolean?][],
    overrides: Record<string, string> = {},
    opts: { shared?: boolean; protected?: boolean } = {},
  ): EnvironmentView => ({
    id: uid(),
    name,
    variables: vars.map(([key, value, secret]) => ({ key, value, enabled: true, secret: !!secret })),
    overrides,
    shared: opts.shared ?? true,
    protected: opts.protected,
  });
  const local = env("Local", [["baseUrl", "http://localhost:8080"], ["customerId", "cus_204"], ["orderId", ""], ["apiToken", "dev-token", true]], { orderId: "ord_841" }, { shared: false });
  const staging = env("Staging", [["baseUrl", "https://api.staging.acme.dev"], ["customerId", "cus_9fA3kL2"], ["orderId", ""], ["apiToken", "stg-token", true]]);
  const production = env("Production", [["baseUrl", "https://api.acme.dev"], ["customerId", "cus_1a77b0"], ["apiToken", "prod-token", true]], {}, { protected: true });

  const mk = (name: string, team: string | null, path: string, collections: MockGroup[], environments: EnvironmentView[]): MockWorkspace => {
    const id = uid();
    return {
      entry: {
        id,
        name,
        path,
        team,
        sync: { enabled: true, intervalSecs: 60, autoPull: true, remote: "origin" },
        prefs: { bookmarks: [], scopedCollection: null, activeEnvironment: environments[0]?.id ?? null },
      },
      manifest: { version: 1, id, name, team: team ?? undefined },
      collections,
      environments,
      saved: {},
      runtime: {},
      history: [],
    };
  };
  const payments = mk("Payments", "Payments Team", "/home/dev/payments-service/api-tests", [orders, customers, auth, webhooks], [local, staging, production]);
  payments.entry.prefs.bookmarks = [orders.meta.id];
  payments.runtime = { traceId: "t-91c2" };
  const inDays = (d: number) => new Date(Date.now() + d * 86_400_000).toISOString().replace(/\.\d+Z$/, "Z");
  jars.set(payments.entry.id, [
    { name: "sid", value: "s_7f3a9c21e04b", domain: "localhost", includeSubdomains: false, path: "/", secure: false, httpOnly: true, sameSite: "Lax" },
    { name: "csrftoken", value: "Xq9vK2mP8rT4", domain: "localhost", includeSubdomains: false, path: "/", expires: inDays(30), secure: false, httpOnly: false, sameSite: "Strict" },
    { name: "__cf_bm", value: "pV3.kQ8-1728480000-1.0.1.1", domain: "acme.dev", includeSubdomains: true, path: "/", expires: inDays(0.02), secure: true, httpOnly: true, sameSite: "None" },
    { name: "staging_session", value: "eyJ1aWQiOiJjdXNfOWZBM2tMMiJ9", domain: "api.staging.acme.dev", includeSubdomains: false, path: "/v1", expires: inDays(7), secure: true, httpOnly: true },
  ]);
  const day = 86_400_000;
  const h = (name: string, method: string, status: number, ago: number): HistoryEntry => ({
    id: uid(),
    name,
    method,
    url: "http://localhost:8080/v1/orders",
    status,
    timeMs: 30 + Math.round(Math.random() * 90),
    ok: status < 400,
    testsPassed: status < 400 ? 1 : 0,
    testsFailed: status < 400 ? 0 : 1,
    environment: "Local",
    at: new Date(Date.now() - ago).toISOString(),
  });
  payments.history = [
    h("List orders", "GET", 200, 4 * 60_000),
    h("Create order", "POST", 201, 9 * 60_000),
    h("Get order", "GET", 404, 26 * 60_000),
    h("Cancel order", "DELETE", 204, 50 * 60_000),
    h("Issue access token", "POST", 200, day + 3_600_000),
  ];

  const ledger = mk("Ledger Sync Squad", "Ledger Sync Squad", "/home/dev/ledger/collections", [group("Ledger API", [r(req("List entries", "GET", "{{baseUrl}}/entries"))], { seq: 1 })], []);
  const northwind = mk("Northwind (client)", "Northwind", "/home/dev/clients/northwind", [group("Northwind API", [r(req("Health", "GET", "{{baseUrl}}/health"))], { seq: 1 })], []);
  const personal = mk("Personal", null, "/home/dev/scratch", [], []);
  return [payments, ledger, northwind, personal];
}

const db = {
  workspaces: seed(),
  active: null as string | null,
  permissions: {
    editRequests: true,
    runRequests: true,
    readResponses: true,
    editEnvironments: false,
    readSecrets: false,
  } as AgentPermissions,
  activity: [
    { kind: "created", what: "Request Refund order in Orders API / Admin", client: "Claude Code", at: new Date(Date.now() - 2 * 60_000).toISOString() },
    { kind: "edited", what: "Request Create order in Orders API / Orders", client: "Claude Code", at: new Date(Date.now() - 4 * 60_000).toISOString() },
    { kind: "ran", what: "4 requests in Orders API, 4 passed", client: "Claude Code", at: new Date(Date.now() - 6 * 60_000).toISOString() },
    { kind: "edited", what: "Environment Local", client: "Cursor", at: new Date(Date.now() - 62 * 60_000).toISOString() },
  ] as AgentActivity[],
};
db.active = db.workspaces[0].entry.id;

function ws(id: unknown): MockWorkspace {
  const w = db.workspaces.find((x) => x.entry.id === id);
  if (!w) throw new Error(`workspace ${String(id)} not found`);
  return w;
}

function toNode(g: MockGroup, dir: string): GroupNode {
  const slug = g.meta.name.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  const here = `${dir}/${slug}`;
  const items: TreeItem[] = g.items.map((it) =>
    it.kind === "folder"
      ? { kind: "folder", ...toNode(it.group, here) }
      : {
          kind: "request",
          id: it.request.id,
          name: it.request.name,
          method: it.request.method,
          url: it.request.url,
          seq: it.request.seq,
          path: `${here}/${it.request.name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}.req.yaml`,
        },
  );
  return { meta: structuredClone(g.meta), dir: here, items };
}

function findGroup(w: MockWorkspace, id: string): { group: MockGroup; parent: MockGroup | null } | null {
  const walk = (g: MockGroup, parent: MockGroup | null): { group: MockGroup; parent: MockGroup | null } | null => {
    if (g.meta.id === id) return { group: g, parent };
    for (const it of g.items) {
      if (it.kind === "folder") {
        const found = walk(it.group, g);
        if (found) return found;
      }
    }
    return null;
  };
  for (const c of w.collections) {
    const found = walk(c, null);
    if (found) return found;
  }
  return null;
}

function findRequest(w: MockWorkspace, id: string): { request: Request; parent: MockGroup; chain: MockGroup[] } | null {
  const walk = (g: MockGroup, chain: MockGroup[]): { request: Request; parent: MockGroup; chain: MockGroup[] } | null => {
    for (const it of g.items) {
      if (it.kind === "request" && it.request.id === id) return { request: it.request, parent: g, chain: [...chain, g] };
      if (it.kind === "folder") {
        const found = walk(it.group, [...chain, g]);
        if (found) return found;
      }
    }
    return null;
  };
  for (const c of w.collections) {
    const found = walk(c, []);
    if (found) return found;
  }
  return null;
}

const delay = (ms: number) => new Promise((res) => setTimeout(res, ms));

// ---------------------------------------------------------------- OAuth 2.0

const oauthTokens = new Map<string, OAuthToken>();
let oauthCancel: (() => void) | null = null;

function effectiveOAuth(w: MockWorkspace, request: Request, parentId: string | null): OAuth2 {
  let chain = findRequest(w, request.id)?.chain;
  if (!chain && parentId) {
    chain = [];
    let at = findGroup(w, parentId);
    while (at) {
      chain.unshift(at.group);
      at = at.parent ? findGroup(w, at.parent.meta.id) : null;
    }
  }
  const auth =
    request.auth && request.auth.type !== "inherit"
      ? request.auth
      : [...(chain ?? [])]
          .reverse()
          .map((g) => g.meta.auth)
          .find((a) => a && a.type !== "inherit");
  if (auth?.type !== "oauth2") throw new Error("this request doesn't use OAuth 2.0 (its own auth or a parent's)");
  return auth;
}

const oauthKey = (workspaceId: string, o: OAuth2) => [workspaceId, o.grant, o.tokenUrl, o.clientId, o.scope ?? "", o.username ?? ""].join(" ");

function cookieHeaderFor(workspaceId: string, url: string): string | null {
  let u: URL;
  try {
    u = new URL(url.includes("://") ? url : `http://${url}`);
  } catch {
    return null;
  }
  const found = jar(workspaceId).filter((c) => {
    const host = u.hostname;
    const domainOk = host === c.domain || (c.includeSubdomains && host.endsWith(`.${c.domain}`));
    const pathOk = c.path === "/" || u.pathname === c.path || u.pathname.startsWith(c.path.replace(/\/?$/, "/"));
    return domainOk && pathOk && (!c.secure || u.protocol === "https:") && (!c.expires || Date.parse(c.expires) > Date.now());
  });
  return found.length ? found.map((c) => `${c.name}=${c.value}`).join("; ") : null;
}


function fakeRun(w: MockWorkspace, request: Request, chain: MockGroup[], envId: string | null): RunResult {
  const env = w.environments.find((e) => e.id === envId);
  const vars: Record<string, string> = {};
  chain[0]?.meta.variables?.forEach((v) => (vars[v.key] = v.value));
  env?.variables.forEach((v) => v.enabled && (vars[v.key] = v.value));
  Object.assign(vars, env?.overrides ?? {}, w.runtime);
  const sub = (s: string) =>
    s.replace(/\{\{\s*([^{}\s]+)\s*\}\}/g, (m, k: string) => (k === "$uuid" ? uid() : (vars[k] ?? m)));
  let url = sub(request.url);
  const params = (request.params ?? []).filter((p) => p.enabled && p.key);
  if (params.length) url += (url.includes("?") ? "&" : "?") + params.map((p) => `${p.key}=${sub(p.value)}`).join("&");
  const headers = [...chain.flatMap((g) => g.meta.headers ?? []), ...(request.headers ?? [])]
    .filter((h) => h.enabled)
    .map((h) => ({ key: h.key, value: sub(h.value) }));
  const body = request.body && "text" in request.body ? sub(request.body.text) : undefined;
  const status = request.method === "POST" ? 201 : request.method === "DELETE" ? 204 : 200;
  const effective = request.auth && request.auth.type !== "inherit" ? request.auth : [...chain].reverse().map((g) => g.meta.auth).find((a) => a && a.type !== "inherit");
  if (effective?.type === "oauth2" && !headers.some((h) => h.key.toLowerCase() === "authorization")) {
    const token = oauthTokens.get(oauthKey(w.entry.id, effective));
    if (token) headers.push({ key: "Authorization", value: `${effective.headerPrefix || "Bearer"} ${token.accessToken}` });
    else if (effective.grant === "authorization-code" || effective.grant === "device-code") {
      return {
        id: uid(),
        requestId: request.id || null,
        requestName: request.name,
        startedAt: new Date().toISOString(),
        request: { method: request.method, url, headers, body },
        response: null,
        error: "OAuth 2.0: no access token yet. Open the Authorization tab and click Sign in.",
        tests: [],
        logs: [],
        scriptErrors: [],
        skipped: false,
      };
    }
  }
  const useJar = request.settings?.cookies !== false && !headers.some((h) => h.key.toLowerCase() === "cookie");
  const cookie = useJar ? cookieHeaderFor(w.entry.id, url) : null;
  if (cookie) headers.push({ key: "Cookie", value: cookie });
  const setCookies: string[] = [];
  if (useJar && request.method === "POST" && /\/(login|sessions?|signin)\b/.test(url)) {
    const host = (() => {
      try {
        return new URL(url).hostname;
      } catch {
        return "localhost";
      }
    })();
    const value = `s_${uid().slice(0, 12)}`;
    setCookies.push(`sid=${value}; Path=/; HttpOnly; SameSite=Lax`);
    const j = jar(w.entry.id);
    const c: CookieInfo = { name: "sid", value, domain: host, includeSubdomains: false, path: "/", secure: false, httpOnly: true, sameSite: "Lax" };
    const at = j.findIndex((x) => sameCookie(x, c));
    if (at >= 0) j[at] = c;
    else j.push(c);
  }
  const scripts = [...chain.map((g) => g.meta.scripts?.postResponse ?? ""), request.scripts?.postResponse ?? ""];
  const tests = scripts.flatMap((s, i) =>
    [...s.matchAll(/(?:trax|pm)\.test\(\s*["'`](.+?)["'`]/g)].map((m) => ({
      name: m[1],
      passed: true,
      source: i < chain.length ? (i === 0 ? `collection:${chain[0].meta.name}` : `folder:${chain[i].meta.name}`) : "request",
    })),
  );
  const html = /\/receipt\b/.test(url) || headers.some((h) => h.key.toLowerCase() === "accept" && h.value.includes("text/html"));
  const responseBody =
    status === 204
      ? ""
      : html
        ? '<!doctype html><html><body style="font-family: Georgia, serif; padding: 24px 28px"><h1>Order ord_841</h1><p>Status: pending · Total €24.00</p><p><a href="https://example.com/receipt.pdf">Download receipt</a></p><script>document.body.append("scripts run?")</script></body></html>'
      : JSON.stringify(
          {
            id: "ord_" + Math.floor(Math.random() * 1000),
            mock: true,
            note: "Preview mode: this response is generated by the in-browser mock backend. Run the desktop app to send real requests.",
            echo: { method: request.method, url, headers: Object.fromEntries(headers.map((h) => [h.key, h.value])), body: body ? safeJson(body) : null },
            items: [],
          },
          null,
          2,
        );
  return {
    id: uid(),
    requestId: request.id || null,
    requestName: request.name,
    startedAt: new Date().toISOString(),
    request: { method: request.method, url, headers, body },
    response: {
      status,
      statusText: status === 201 ? "Created" : status === 204 ? "No Content" : "OK",
      httpVersion: "HTTP/1.1",
      headers: [
        { key: "content-type", value: html ? "text/html; charset=utf-8" : "application/json" },
        { key: "x-request-id", value: uid() },
        { key: "date", value: new Date().toUTCString() },
        ...setCookies.map((value) => ({ key: "set-cookie", value })),
      ],
      body: responseBody,
      size: responseBody.length,
      timeMs: 20 + Math.floor(Math.random() * 180),
      url,
    },
    error: null,
    tests,
    logs: [{ level: "info", message: "mock backend: request not actually sent", phase: "prepare", source: "runner" }],
    scriptErrors: [],
    skipped: false,
  };
}

function safeJson(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return s;
  }
}

const now = () => Math.floor(Date.now() / 1000);

const CONFLICT_PATH = "api-tests/collections/orders-api/orders/create-order.req.yaml";

const gitState = {
  branch: "feature/refunds",
  merging: false,
  files: [
    { path: "api-tests/environments/local.env.yaml", index: "M", worktree: ".", staged: true, unstaged: false, untracked: false, conflicted: false, inWorkspace: true },
    { path: "api-tests/collections/orders-api/orders/create-order.req.yaml", index: ".", worktree: "M", staged: false, unstaged: true, untracked: false, conflicted: false, inWorkspace: true },
    { path: "api-tests/collections/orders-api/orders/refund-order.req.yaml", index: "?", worktree: "?", staged: false, unstaged: true, untracked: true, conflicted: false, inWorkspace: true },
  ],
  commits: [
    { subject: "Add refund flow requests", author: "Ada Lovelace", ago: 3600 },
    { subject: "Assert idempotency on create order", author: "Grace Hopper", ago: 7200 },
    { subject: "Rename Orders folder", author: "Ada Lovelace", ago: 18000 },
    { subject: "Merge 'feature/customers' into main", author: "Linus T.", ago: 86400, merge: true },
    { subject: "Add customers API collection", author: "Linus T.", ago: 172800 },
    { subject: "Initial import from Postman", author: "Ada Lovelace", ago: 432000 },
  ],
};

const FILE_CHANGES: Record<string, Omit<FileChanges, "path" | "diff">> = {
  "local.env.yaml": {
    kind: "environment",
    additions: 2,
    deletions: 2,
    changes: [
      { kind: "added", label: "Variable added", subject: "apiToken", area: "Variable", after: "secret · not synced" },
      { kind: "updated", label: "Variable updated", subject: "baseUrl", area: "Variable", before: "http://localhost:3000", after: "http://localhost:8080" },
      { kind: "removed", label: "Variable removed", subject: "debug", area: "Variable", before: "true" },
    ],
  },
  "create-order.req.yaml": {
    kind: "request",
    additions: 6,
    deletions: 2,
    changes: [
      { kind: "updated", label: "URL changed", subject: "POST", area: "Request", before: "{{baseUrl}}/v1/orders", after: "{{baseUrl}}/v2/orders" },
      { kind: "added", label: "Header added", subject: "Idempotency-Key", area: "Headers", after: "{{$uuid}}" },
      { kind: "updated", label: "Body updated", subject: "currency", area: "Body", before: '"USD"', after: '"EUR"' },
      { kind: "added", label: "Test added", subject: "status is 201", area: "Scripts", after: "trax.expect(trax.response.status).toBe(201)" },
    ],
  },
  "refund-order.req.yaml": {
    kind: "request",
    additions: 9,
    deletions: 0,
    changes: [
      { kind: "added", label: "Request added", subject: "POST {{baseUrl}}/v1/orders/{{orderId}}/refund", area: "Request" },
      { kind: "added", label: "Body added", subject: "json", area: "Body", after: '{"amount":2400,"reason":"duplicate"}' },
    ],
  },
};

const SAMPLE_DIFF = `diff --git a/api-tests/collections/orders-api/orders/create-order.req.yaml b/api-tests/collections/orders-api/orders/create-order.req.yaml
index 3b18e51..a9c4f2d 100644
--- a/api-tests/collections/orders-api/orders/create-order.req.yaml
+++ b/api-tests/collections/orders-api/orders/create-order.req.yaml
@@ -4,10 +4,13 @@ seq: 2
 method: POST
 url: '{{baseUrl}}/v1/orders'
+headers:
+- key: Idempotency-Key
+  value: '{{$uuid}}'
 body:
   mode: json
   text: |-
     {
-      "customerId": "cus_123",
+      "customerId": "{{customerId}}",
       "items": [
         { "sku": "TRX-001", "quantity": 2 }
       ],
`;

const listeners = new Map<string, Set<(p: unknown) => void>>();
let teammatePushed = false;

export function mockListen(name: string, handler: (p: unknown) => void): () => void {
  if (!listeners.has(name)) listeners.set(name, new Set());
  listeners.get(name)!.add(handler);
  return () => listeners.get(name)?.delete(handler);
}

function syncReport(w: MockWorkspace): SyncReport {
  return {
    state: w === db.workspaces[0] ? "ahead" : "synced",
    branch: w === db.workspaces[0] ? gitState.branch : "main",
    upstream: w === db.workspaces[0] ? `origin/${gitState.branch}` : "origin/main",
    ahead: w === db.workspaces[0] ? 1 : 0,
    behind: 0,
    changes: w === db.workspaces[0] ? gitState.files.length : 0,
    pulled: 0,
    fetchedAt: new Date(Date.now() - 12_000).toISOString(),
    checkedAt: new Date().toISOString(),
    message: null,
  };
}

function recordHistory(w: MockWorkspace, run: RunResult, envId: string | null) {
  w.history.unshift({
    id: uid(),
    requestId: run.requestId,
    name: run.requestName,
    method: run.request.method,
    url: run.request.url,
    status: run.response?.status ?? null,
    timeMs: run.response?.timeMs ?? null,
    ok: !run.error && run.tests.every((t) => t.passed),
    testsPassed: run.tests.filter((t) => t.passed).length,
    testsFailed: run.tests.filter((t) => !t.passed).length,
    environment: w.environments.find((e) => e.id === envId)?.name ?? null,
    at: run.startedAt,
  });
}

type Args = Record<string, unknown>;
const s = (a: Args, k: string) => a[k] as string;

/** A small subset of the Rust importer, enough for the browser preview. */
function importSpecMock(w: MockWorkspace, raw: string) {
  let v: Record<string, unknown>;
  try {
    v = JSON.parse(raw);
  } catch (e) {
    if (raw.trim().startsWith("openapi:") || raw.trim().startsWith("swagger:")) {
      throw new Error("YAML parsing is supported in the desktop app. In the browser preview, please use JSON.");
    }
    throw new Error(`not valid JSON: ${(e as Error).message}`);
  }
  const summary = { collections: [] as string[], environments: [] as string[], requests: 0, warnings: [] as string[] };
  type PmItem = { name?: string; item?: PmItem[]; request?: { method?: string; url?: string | { raw?: string } } | string };
  const toItems = (items: PmItem[]): MockItem[] =>
    items.map((it) => {
      if (it.item) return f(group(it.name || "Folder", toItems(it.item)));
      const rq = it.request;
      const method = typeof rq === "string" ? "GET" : (rq?.method || "GET").toUpperCase();
      const url = typeof rq === "string" ? rq : typeof rq?.url === "string" ? rq.url : (rq?.url?.raw ?? "");
      summary.requests++;
      return r(req(it.name || "Untitled request", method, url.split("?")[0]));
    });

  if (v.info && Array.isArray(v.item)) {
    const info = v.info as { name?: string };
    const c = group(info.name || "Imported collection", toItems(v.item as PmItem[]), { seq: w.collections.length + 1 });
    w.collections.push(c);
    summary.collections.push(c.meta.id);
  } else if (Array.isArray(v.values)) {
    const values = v.values as { key: string; value?: string; enabled?: boolean; type?: string }[];
    const name = (v.name as string) || "Imported environment";
    w.environments.push({
      id: uid(),
      name,
      variables: values.map((x) => ({ key: x.key, value: String(x.value ?? ""), enabled: x.enabled !== false, secret: x.type === "secret" })),
      overrides: {},
      shared: true,
    });
    summary.environments.push(name);
  } else if (typeof v.openapi === "string" || typeof v.swagger === "string") {
    const info = (v.info ?? {}) as { title?: string; version?: string };
    const paths = (v.paths ?? {}) as Record<string, Record<string, unknown>>;
    const servers = Array.isArray(v.servers) ? (v.servers as { url?: string }[]) : [];
    const basePath = servers[0]?.url || (typeof v.basePath === "string" ? v.basePath : "");
    const items: MockItem[] = [];
    const methods = ["get", "post", "put", "delete", "patch", "options", "head"];
    for (const [pathStr, pathObj] of Object.entries(paths)) {
      if (!pathObj || typeof pathObj !== "object") continue;
      for (const m of methods) {
        const op = pathObj[m] as { summary?: string; operationId?: string } | undefined;
        if (!op) continue;
        const name = op.summary || op.operationId || `${m.toUpperCase()} ${pathStr}`;
        const urlPath = pathStr.replace(/\{([^}]+)\}/g, "{{$1}}");
        const fullUrl = basePath.endsWith("/") && urlPath.startsWith("/") ? basePath + urlPath.slice(1) : basePath + urlPath;
        summary.requests++;
        items.push(r(req(name, m.toUpperCase(), fullUrl)));
      }
    }
    const c = group(info.title || "OpenAPI Specification", items, { seq: w.collections.length + 1 });
    w.collections.push(c);
    summary.collections.push(c.meta.id);
  } else if (v.log && typeof v.log === "object" && Array.isArray((v.log as { entries?: unknown[] }).entries)) {
    const entries = (v.log as { entries: { request?: { method?: string; url?: string } }[] }).entries;
    const items: MockItem[] = [];
    for (const entry of entries) {
      const rq = entry.request;
      if (!rq?.url) continue;
      const method = (rq.method || "GET").toUpperCase();
      let name: string;
      try {
        const parsed = new URL(rq.url);
        name = `${method} ${parsed.pathname}`;
      } catch {
        name = `${method} ${rq.url}`;
      }
      summary.requests++;
      items.push(r(req(name, method, rq.url.split("?")[0])));
    }
    const c = group("HAR Import", items, { seq: w.collections.length + 1 });
    w.collections.push(c);
    summary.collections.push(c.meta.id);
  } else if (v.__export_format === 4 || (Array.isArray(v.resources) && (v.resources as { _type?: string }[]).some((r) => r._type === "request"))) {
    const resources = v.resources as { _type?: string; name?: string; method?: string; url?: string; data?: Record<string, unknown> }[];
    const items: MockItem[] = [];
    let collName = "Insomnia Import";
    for (const res of resources) {
      if (res._type === "workspace" && res.name) {
        collName = res.name;
      } else if (res._type === "request") {
        summary.requests++;
        items.push(r(req(res.name || "Untitled request", (res.method || "GET").toUpperCase(), res.url || "")));
      } else if (res._type === "environment" && res.data) {
        const envName = res.name || "Insomnia Environment";
        w.environments.push({
          id: uid(),
          name: envName,
          variables: Object.entries(res.data).map(([k, val]) => ({ key: k, value: String(val ?? ""), enabled: true })),
          overrides: {},
          shared: true,
        });
        summary.environments.push(envName);
      }
    }
    if (items.length > 0) {
      const c = group(collName, items, { seq: w.collections.length + 1 });
      w.collections.push(c);
      summary.collections.push(c.meta.id);
    }
  } else if (Array.isArray(v.items) && (typeof v.name === "string" || v.version === "1")) {
    const collName = (v.name as string) || "Bruno Collection";
    type BrunoItem = { name?: string; items?: BrunoItem[]; request?: { method?: string; url?: string } };
    const toBrunoItems = (arr: BrunoItem[]): MockItem[] => {
      return arr.map((item) => {
        if (Array.isArray(item.items)) {
          return f(group(item.name || "Folder", toBrunoItems(item.items)));
        }
        const rq = item.request;
        const method = (rq?.method || "GET").toUpperCase();
        const url = rq?.url || "";
        summary.requests++;
        return r(req(item.name || "Untitled request", method, url.split("?")[0]));
      });
    };
    const c = group(collName, toBrunoItems(v.items as BrunoItem[]), { seq: w.collections.length + 1 });
    w.collections.push(c);
    summary.collections.push(c.meta.id);
  } else {
    throw new Error("unrecognized format: expected OpenAPI 3.x / Swagger, Postman (v2.0/v2.1), HAR, Insomnia, or Bruno export");
  }
  return summary;
}

const handlers: Record<string, (a: Args) => unknown> = {
  app_info: () => ({ version: "0.1.0-preview", gitVersion: "git version 2.47.0", mcpCommand: "/usr/bin/trax-mcp", configDir: "~/.config/http-trax", platform: "browser" }),
  list_workspaces: () => ({ workspaces: db.workspaces.map((w) => structuredClone(w.entry)), activeWorkspace: db.active }),
  set_active_workspace: (a) => void (db.active = s(a, "id")),
  update_workspace_prefs: (a) => void (ws(a.id).entry.prefs = structuredClone(a.prefs) as WorkspaceEntry["prefs"]),
  update_sync_settings: (a) => void (ws(a.id).entry.sync = structuredClone(a.sync) as WorkspaceEntry["sync"]),
  update_manifest: (a) => {
    const w = ws(a.id);
    w.manifest = structuredClone(a.manifest) as WorkspaceManifest;
    w.entry.name = w.manifest.name;
    w.entry.team = w.manifest.team ?? null;
  },
  create_workspace: (a) => {
    const id = uid();
    const tpl = (a.template as string | null) ?? null;
    const collections: MockGroup[] = [];
    const environments: EnvironmentView[] = [];
    if (tpl && tpl !== "blank") {
      environments.push({
        id: uid(),
        name: "Local",
        variables: [{ key: "baseUrl", value: "http://localhost:8080", enabled: true }],
        overrides: {},
        shared: true,
      });
      if (tpl === "rest-crud") {
        collections.push(
          group("Items API", [
            r(req("List items", "GET", "{{baseUrl}}/v1/items")),
            r(req("Create item", "POST", "{{baseUrl}}/v1/items")),
            r(req("Get item", "GET", "{{baseUrl}}/v1/items/{{itemId}}")),
            r(req("Delete item", "DELETE", "{{baseUrl}}/v1/items/{{itemId}}")),
          ], { seq: 1 }),
        );
      } else if (tpl === "graphql-service") {
        collections.push(
          group("GraphQL API", [
            r(req("Viewer profile", "POST", "{{baseUrl}}/graphql", { body: { mode: "graphql", query: "query Viewer { viewer { id name } }", variables: "{}" } })),
          ], { seq: 1 }),
        );
      } else if (tpl === "oauth2-microservice") {
        collections.push(
          group("Service API", [r(req("Health check", "GET", "{{baseUrl}}/health"))], { seq: 1 }),
        );
      } else if (tpl === "webhook-suite") {
        collections.push(
          group("Webhooks", [r(req("Deliver event", "POST", "{{baseUrl}}/webhooks/events"))], { seq: 1 }),
        );
      }
    }
    const w: MockWorkspace = {
      entry: {
        id,
        name: s(a, "name"),
        path: s(a, "path"),
        team: (a.team as string) || null,
        sync: { enabled: true, intervalSecs: 60, autoPull: true, remote: "origin" },
        prefs: { bookmarks: [], activeEnvironment: environments[0]?.id ?? null },
      },
      manifest: { version: 1, id, name: s(a, "name"), team: (a.team as string) || undefined },
      collections,
      environments,
      saved: {},
      runtime: {},
      history: [],
    };
    db.workspaces.push(w);
    db.active = id;
    return structuredClone(w.entry);
  },
  open_workspace: () => {
    throw new Error("Opening folders needs the desktop app (preview mode).");
  },
  clone_workspace: () => {
    throw new Error("Cloning needs the desktop app (preview mode).");
  },
  remove_workspace: (a) => {
    db.workspaces = db.workspaces.filter((w) => w.entry.id !== a.id);
    if (db.active === a.id) db.active = db.workspaces[0]?.entry.id ?? null;
  },
  inspect_path: () => "missing",
  load_workspace: (a) => {
    const w = ws(a.id);
    return {
      manifest: structuredClone(w.manifest),
      root: w.entry.path,
      collections: w.collections.map((c) => toNode(c, "collections")),
      environments: structuredClone(w.environments),
      problems: [],
    };
  },
  create_collection: (a) => {
    const w = ws(a.workspaceId);
    const g = group(s(a, "name"), [], { seq: w.collections.length + 1 });
    w.collections.push(g);
    return toNode(g, "collections");
  },
  create_folder: (a) => {
    const w = ws(a.workspaceId);
    const parent = findGroup(w, s(a, "parentId"));
    if (!parent) throw new Error("parent not found");
    const g = group(s(a, "name"), [], { seq: parent.group.items.length + 1 });
    parent.group.items.push(f(g));
    return toNode(g, "collections");
  },
  get_group: (a) => structuredClone(findGroup(ws(a.workspaceId), s(a, "id"))!.group.meta),
  update_group: (a) => {
    const found = findGroup(ws(a.workspaceId), (a.meta as GroupMeta).id);
    if (!found) throw new Error("not found");
    found.group.meta = structuredClone(a.meta as GroupMeta);
  },
  delete_group: (a) => {
    const w = ws(a.workspaceId);
    const id = s(a, "id");
    w.collections = w.collections.filter((c) => c.meta.id !== id);
    const strip = (g: MockGroup) => {
      g.items = g.items.filter((it) => !(it.kind === "folder" && it.group.meta.id === id));
      g.items.forEach((it) => it.kind === "folder" && strip(it.group));
    };
    w.collections.forEach(strip);
    w.entry.prefs.bookmarks = w.entry.prefs.bookmarks.filter((b) => b !== id);
    if (w.entry.prefs.scopedCollection === id) w.entry.prefs.scopedCollection = null;
  },
  get_request: (a) => {
    const found = findRequest(ws(a.workspaceId), s(a, "id"));
    if (!found) throw new Error("request not found");
    return structuredClone(found.request);
  },
  create_request: (a) => {
    const w = ws(a.workspaceId);
    const parent = findGroup(w, s(a, "parentId"));
    if (!parent) throw new Error("parent not found");
    const request = structuredClone(a.request as Request);
    if (!request.id) request.id = uid();
    request.seq = parent.group.items.length + 1;
    parent.group.items.push(r(request));
    return structuredClone(request);
  },
  save_request: (a) => {
    const request = a.request as Request;
    const found = findRequest(ws(a.workspaceId), request.id);
    if (!found) throw new Error("request not found");
    Object.assign(found.request, structuredClone(request));
  },
  delete_request: (a) => {
    const found = findRequest(ws(a.workspaceId), s(a, "id"));
    if (found) found.parent.items = found.parent.items.filter((it) => !(it.kind === "request" && it.request.id === a.id));
  },
  duplicate_request: (a) => {
    const found = findRequest(ws(a.workspaceId), s(a, "id"));
    if (!found) throw new Error("request not found");
    const copy = { ...structuredClone(found.request), id: uid(), name: `${found.request.name} copy` };
    found.parent.items.push(r(copy));
    return copy;
  },
  move_request: (a) => {
    const w = ws(a.workspaceId);
    const found = findRequest(w, s(a, "id"));
    const target = findGroup(w, s(a, "parentId"));
    if (!found || !target) throw new Error("not found");
    found.parent.items = found.parent.items.filter((it) => !(it.kind === "request" && it.request.id === a.id));
    target.group.items.push(r(found.request));
  },
  save_environment: (a) => {
    const w = ws(a.workspaceId);
    const env = structuredClone(a.environment as Environment);
    if (!env.id) env.id = uid();
    const existing = w.environments.find((e) => e.id === env.id);
    if (existing) Object.assign(existing, env);
    else w.environments.push({ ...env, overrides: {}, shared: true });
    return env;
  },
  set_environment_shared: (a) => {
    const env = ws(a.workspaceId).environments.find((e) => e.id === a.id);
    if (!env) throw new Error("environment not found");
    env.shared = !!a.shared;
    return structuredClone(env);
  },
  import_json: (a) => importSpecMock(ws(a.workspaceId), s(a, "json")),
  oauth_token_status: (a) => {
    const o = effectiveOAuth(ws(a.workspaceId), a.request as Request, (a.parentId as string | null) ?? null);
    const token = oauthTokens.get(oauthKey(s(a, "workspaceId"), o)) ?? null;
    return {
      token,
      fresh: !!token && (!token.expiresAt || Date.parse(token.expiresAt) - 30_000 > Date.now()),
      needsUser: o.grant === "authorization-code" || o.grant === "device-code",
    };
  },
  oauth_get_token: async (a) => {
    const o = effectiveOAuth(ws(a.workspaceId), a.request as Request, (a.parentId as string | null) ?? null);
    if (!o.tokenUrl) throw new Error("the URL is empty");
    oauthCancel?.();
    const emit = (p: unknown) => listeners.get("oauth-prompt")?.forEach((h) => h(p));
    let wait = 700;
    if (o.grant === "authorization-code") {
      emit({ kind: "browser", url: `${o.authUrl}?response_type=code&client_id=${encodeURIComponent(o.clientId)}&code_challenge_method=S256` });
      wait = 2600;
    } else if (o.grant === "device-code") {
      emit({ kind: "device", userCode: "WDJB-MJHT", verificationUri: "https://id.acme.dev/device", verificationUriComplete: "https://id.acme.dev/device?user_code=WDJB-MJHT", expiresIn: 900 });
      wait = 4000;
    }
    await new Promise<void>((resolve, reject) => {
      const t = setTimeout(resolve, wait);
      oauthCancel = () => {
        clearTimeout(t);
        reject(new Error("sign-in cancelled"));
      };
    });
    oauthCancel = null;
    const now = Date.now();
    const token: OAuthToken = {
      accessToken: `eyJhbGciOiJSUzI1NiIsImtpZCI6Im1vY2sifQ.${btoa(JSON.stringify({ sub: "ada", scope: o.scope ?? "" })).replace(/=+$/, "")}.${uid().slice(0, 12)}`,
      tokenType: "Bearer",
      refreshToken: o.grant === "client-credentials" ? null : `rt_${uid().slice(0, 16)}`,
      expiresAt: new Date(now + 3600_000).toISOString(),
      scope: o.scope || null,
      obtainedAt: new Date(now).toISOString(),
    };
    oauthTokens.set(oauthKey(s(a, "workspaceId"), o), token);
    return token;
  },
  oauth_cancel: () => void oauthCancel?.(),
  oauth_refresh_token: async (a) => {
    const o = effectiveOAuth(ws(a.workspaceId), a.request as Request, (a.parentId as string | null) ?? null);
    const key = oauthKey(s(a, "workspaceId"), o);
    const old = oauthTokens.get(key);
    if (!old?.refreshToken) throw new Error("there is no refresh token");
    await delay(400);
    const token = { ...old, accessToken: `${old.accessToken.split(".").slice(0, 2).join(".")}.${uid().slice(0, 12)}`, expiresAt: new Date(Date.now() + 3600_000).toISOString(), obtainedAt: new Date().toISOString() };
    oauthTokens.set(key, token);
    return token;
  },
  oauth_clear_token: (a) => {
    const o = effectiveOAuth(ws(a.workspaceId), a.request as Request, (a.parentId as string | null) ?? null);
    return oauthTokens.delete(oauthKey(s(a, "workspaceId"), o));
  },
  list_cookies: (a) =>
    [...jar(s(a, "workspaceId"))]
      .filter((c) => !c.expires || Date.parse(c.expires) > Date.now())
      .sort((x, y) => x.domain.localeCompare(y.domain) || x.path.localeCompare(y.path) || x.name.localeCompare(y.name)),
  save_cookie: (a) => {
    const j = jar(s(a, "workspaceId"));
    const c = { ...(a.cookie as CookieInfo) };
    c.domain = c.domain.trim().replace(/^\./, "").toLowerCase();
    c.path = c.path.startsWith("/") ? c.path : `/${c.path}`;
    if (!c.name || /[\s=;,]/.test(c.name)) throw new Error(`invalid cookie name "${c.name}"`);
    if (c.value.includes(";")) throw new Error(`the value of ${c.name} can't contain ";"`);
    if (!c.domain) throw new Error("invalid cookie domain");
    if (c.expires && Date.parse(c.expires) <= Date.now()) throw new Error("the cookie has already expired");
    const prev = a.previous as CookieKey | null;
    const keep = j.filter((x) => !(prev && sameCookie(x, prev)) && !sameCookie(x, c));
    jars.set(s(a, "workspaceId"), [...keep, c]);
    return c;
  },
  delete_cookie: (a) => {
    const j = jar(s(a, "workspaceId"));
    const keep = j.filter((x) => !sameCookie(x, a.key as CookieKey));
    jars.set(s(a, "workspaceId"), keep);
    return keep.length !== j.length;
  },
  clear_cookies: (a) => {
    const j = jar(s(a, "workspaceId"));
    const d = (a.domain as string | null)?.replace(/^\./, "").toLowerCase();
    const keep = d ? j.filter((c) => c.domain !== d && !c.domain.endsWith(`.${d}`)) : [];
    jars.set(s(a, "workspaceId"), keep);
    return j.length - keep.length;
  },
  export_request: (a): ExportRequest => {
    const w = ws(a.workspaceId);
    const request = a.request as Request;
    const chain = findRequest(w, request.id)?.chain ?? [];
    const env = w.environments.find((e) => e.id === a.environmentId);
    const vars: Record<string, string> = {};
    chain[0]?.meta.variables?.forEach((v) => (vars[v.key] = v.value));
    env?.variables.forEach((v) => v.enabled && (vars[v.key] = v.value));
    Object.assign(vars, env?.overrides ?? {}, w.runtime);
    const sub = (t: string) => (a.resolve ? t.replace(/\{\{\s*([^{}\s]+)\s*\}\}/g, (m, k: string) => vars[k] ?? m) : t);
    let url = request.url;
    const params = (request.params ?? []).filter((p) => p.enabled && p.key);
    if (params.length) url += (url.includes("?") ? "&" : "?") + params.map((p) => `${p.key}=${p.value}`).join("&");
    const headers = [...chain.flatMap((g) => g.meta.headers ?? []), ...(request.headers ?? [])]
      .filter((h) => h.enabled && h.key)
      .map((h) => ({ key: h.key, value: sub(h.value) }));
    const b = request.body ?? { mode: "none" };
    if ((b.mode === "json" || b.mode === "graphql") && !headers.some((h) => h.key.toLowerCase() === "content-type")) headers.push({ key: "Content-Type", value: "application/json" });
    if (b.mode === "form-urlencoded") headers.push({ key: "Content-Type", value: "application/x-www-form-urlencoded" });
    const auth = request.auth?.type === "inherit" || !request.auth ? [...chain].reverse().map((g) => g.meta.auth).find((x) => x && x.type !== "inherit") : request.auth;
    let basicAuth = null;
    if (auth?.type === "bearer") headers.push({ key: "Authorization", value: `Bearer ${sub(auth.token)}` });
    if (auth?.type === "basic") basicAuth = { username: sub(auth.username), password: sub(auth.password) };
    if (auth?.type === "api-key" && auth.location === "header") headers.push({ key: auth.key, value: sub(auth.value) });
    const warnings: string[] = [];
    if (auth?.type === "oauth2") {
      const token = a.resolve ? oauthTokens.get(oauthKey(w.entry.id, auth)) : undefined;
      headers.push({ key: "Authorization", value: `${auth.headerPrefix || "Bearer"} ${token?.accessToken ?? "<access token>"}` });
      if (a.resolve && !token) warnings.push("No OAuth 2.0 access token yet: send the request or get one in the Authorization tab, then copy again.");
    }
    const body: ExportRequest["body"] =
      b.mode === "json" || b.mode === "text" || b.mode === "xml"
        ? { kind: "text", text: sub(b.text) }
        : b.mode === "form-urlencoded"
          ? { kind: "form", fields: b.fields.filter((f) => f.enabled).map((f) => [f.key, sub(f.value)] as [string, string]) }
          : b.mode === "multipart"
            ? { kind: "multipart", parts: b.fields.filter((f) => f.enabled).map((f) => ({ name: f.key, value: sub(f.value), isFile: !!f.isFile })) }
            : b.mode === "graphql"
              ? { kind: "text", text: JSON.stringify({ query: b.query, variables: b.variables ? safeJson(b.variables) : null }) }
              : { kind: "none" };
    if (request.scripts?.preRequest?.trim()) warnings.unshift("Pre-request scripts don't run for exports, so anything they set is missing.");
    return { method: request.method.toUpperCase(), url: sub(url), headers, body, basicAuth, warnings };
  },
  agent_overview: (a) => {
    const now = Date.now();
    const session = (pid: number, client: string, clientId: string, ago: number) => ({
      pid,
      client,
      clientId,
      version: "2.1.0",
      startedAt: new Date(now - 40 * 60_000).toISOString(),
      lastSeen: new Date(now - ago).toISOString(),
    });
    const code = session(4112, "Claude Code", "claude-code", 12_000);
    const cursor = session(4380, "Cursor", "cursor-vscode", 25_000);
    return {
      permissions: { ...db.permissions },
      sessions: [code, cursor],
      clients: [
        { id: "code", name: "Claude Code", status: "connected", configPath: "~/.claude.json", session: code },
        { id: "desktop", name: "Claude Desktop, Cursor, Windsurf", status: "connected", configPath: "~/.cursor/mcp.json", session: cursor },
        { id: "vscode", name: "VS Code", status: "not-set-up", configPath: null, session: null },
      ],
      activity: a.workspaceId === db.workspaces[0].entry.id ? structuredClone(db.activity) : [],
    };
  },
  set_agent_permissions: (a) => (db.permissions = { ...(a.permissions as AgentPermissions) }),
  workspace_summaries: () =>
    db.workspaces.map((w, i) => {
      const count = (g: MockGroup): number => g.items.reduce((n, it) => n + (it.kind === "request" ? 1 : count(it.group)), 0);
      const isRepo = i < 3;
      return {
        id: w.entry.id,
        exists: true,
        requests: w.collections.reduce((n, c) => n + count(c), 0),
        collections: w.collections.length,
        isRepo,
        remote: isRepo ? `git@github.com:acme/${w.entry.name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}.git` : null,
        members: [4, 9, 3][i] ?? null,
      };
    }),
  delete_environment: (a) => {
    const w = ws(a.workspaceId);
    w.environments = w.environments.filter((e) => e.id !== a.id);
  },
  set_environment_override: (a) => {
    const env = ws(a.workspaceId).environments.find((e) => e.id === a.environmentId);
    if (!env) return;
    if (a.value === null) delete env.overrides[s(a, "key")];
    else env.overrides[s(a, "key")] = s(a, "value");
  },
  get_local_state: (a) => {
    const w = ws(a.workspaceId);
    return { runtimeVariables: { ...w.runtime }, overrides: Object.fromEntries(w.environments.map((e) => [e.id, { ...e.overrides }])) };
  },
  clear_runtime_variables: (a) => void (ws(a.workspaceId).runtime = {}),
  send_request: async (a) => {
    const w = ws(a.workspaceId);
    const request = a.request as Request;
    const found = findRequest(w, request.id);
    const chain = found?.chain ?? (a.parentId ? [findGroup(w, s(a, "parentId"))?.group].filter(Boolean) as MockGroup[] : []);
    await delay(250 + Math.random() * 300);
    const run = fakeRun(w, request, chain, (a.environmentId as string) ?? null);
    recordHistory(w, run, (a.environmentId as string) ?? null);
    return run;
  },
  run_collection: async (a) => {
    const w = ws(a.workspaceId);
    const g = findGroup(w, s(a, "groupId"));
    if (!g) throw new Error("not found");
    const all: { request: Request; chain: MockGroup[] }[] = [];
    const walk = (grp: MockGroup, chain: MockGroup[]) =>
      grp.items.forEach((it) => (it.kind === "request" ? all.push({ request: it.request, chain: [...chain, grp] }) : walk(it.group, [...chain, grp])));
    walk(g.group, []);
    await delay(600);
    const results = all.map((x) => fakeRun(w, x.request, x.chain, (a.environmentId as string) ?? null));
    results.forEach((run) => recordHistory(w, run, (a.environmentId as string) ?? null));
    const items = results.map((res, i) => ({
      requestId: all[i].request.id,
      name: all[i].request.name,
      method: all[i].request.method,
      url: res.request.url,
      status: res.response?.status,
      timeMs: res.response?.timeMs,
      passed: res.tests.length,
      failed: 0,
      error: null,
      ok: true,
    }));
    return {
      report: {
        groupId: g.group.meta.id,
        groupName: g.group.meta.name,
        environment: w.environments.find((e) => e.id === a.environmentId)?.name ?? null,
        items,
        total: items.length,
        succeeded: items.length,
        failed: 0,
        testsPassed: items.reduce((n, i) => n + i.passed, 0),
        testsFailed: 0,
        durationMs: items.reduce((n, i) => n + (i.timeMs ?? 0), 0),
      },
      results,
    };
  },
  list_saved_responses: (a) => structuredClone(ws(a.workspaceId).saved[s(a, "requestId")] ?? []),
  save_response: (a) => {
    const w = ws(a.workspaceId);
    const run = a.run as RunResult;
    const saved: SavedResponse = {
      id: uid(),
      requestId: s(a, "requestId"),
      name: s(a, "name") || `${run.response!.status} ${run.response!.statusText}`,
      savedAt: new Date().toISOString(),
      request: run.request,
      response: run.response!,
    };
    (w.saved[saved.requestId] ??= []).unshift(saved);
    return saved;
  },
  delete_saved_response: (a) => {
    const w = ws(a.workspaceId);
    w.saved[s(a, "requestId")] = (w.saved[s(a, "requestId")] ?? []).filter((x) => x.id !== a.responseId);
  },

  // git
  git_status: (a): RepoStatus => {
    const primary = ws(a.workspaceId) === db.workspaces[0];
    return {
      isRepo: true,
      root: "/home/dev/payments-service",
      prefix: "api-tests",
      branch: primary ? gitState.branch : "main",
      detached: false,
      head: "a9c4f2d",
      upstream: primary ? `origin/${gitState.branch}` : "origin/main",
      ahead: primary ? 1 : 0,
      behind: 0,
      files: primary ? structuredClone(gitState.files).map((x) => ({ ...x, origPath: null })) : [],
      merging: primary && gitState.merging,
      rebasing: false,
      conflicts: primary ? gitState.files.filter((f) => f.conflicted).length : 0,
      hasRemote: true,
    };
  },
  git_init: () => undefined,
  git_stage: (a) => {
    const paths = a.paths as string[];
    gitState.files.forEach((x) => {
      if (paths.length === 0 || paths.includes(x.path)) {
        x.staged = true;
        x.unstaged = false;
        x.untracked = false;
        x.index = x.index === "?" ? "A" : "M";
        x.worktree = ".";
      }
    });
  },
  git_unstage: (a) => {
    const paths = a.paths as string[];
    gitState.files.forEach((x) => {
      if (paths.length === 0 || paths.includes(x.path)) {
        x.staged = false;
        x.unstaged = true;
        x.worktree = x.index === "A" ? "?" : "M";
        x.untracked = x.index === "A";
        x.index = x.untracked ? "?" : ".";
      }
    });
  },
  git_discard: (a) => void (gitState.files = gitState.files.filter((x) => !(a.paths as string[]).includes(x.path))),
  git_commit: (a): Commit => {
    gitState.files = gitState.files.filter((x) => !x.staged);
    gitState.commits.unshift({ subject: s(a, "message"), author: "You", ago: 0 });
    return {
      hash: uid(),
      shortHash: uid().slice(0, 7),
      authorName: "You",
      authorEmail: "you@example.com",
      date: now(),
      subject: s(a, "message"),
      parents: [],
      refs: [],
      ...(mockSigning.enabled ? { signatureStatus: "good" as const, signer: "Ada Lovelace <ada@payments.example.com>" } : {}),
    };
  },
  git_fetch: () => delay(400),
  git_pull: async () => {
    await delay(400);
    return { kind: "up-to-date" };
  },
  git_push: () => delay(500),
  git_branches: (): Branch[] => {
    const b = (name: string, remote: boolean, current = false, ahead = 0, behind = 0): Branch => ({ name, remote, current, ahead, behind, upstream: remote ? null : `origin/${name}`, lastCommit: "Add refund flow requests", lastCommitDate: now() - 3600 });
    return [
      b("main", false),
      b(gitState.branch, false, true, 1),
      b("origin/main", true),
      b(`origin/${gitState.branch}`, true),
      b("origin/feature/webhooks", true),
    ];
  },
  git_create_branch: (a) => void (a.checkout && (gitState.branch = s(a, "name"))),
  git_checkout: (a) => void (gitState.branch = s(a, "name").replace(/^origin\//, "")),
  git_delete_branch: () => undefined,
  git_merge: async (a) => {
    await delay(300);
    // Merging main brings in a teammate's edit to the same header.
    if (a.branch !== "main") return { kind: "merged" };
    gitState.merging = true;
    gitState.files = gitState.files.filter((f) => f.path !== CONFLICT_PATH);
    gitState.files.unshift({ path: CONFLICT_PATH, index: "U", worktree: "U", staged: false, unstaged: true, untracked: false, conflicted: true, inWorkspace: true });
    return { kind: "conflicts", files: [CONFLICT_PATH] };
  },
  git_abort: () => {
    gitState.merging = false;
    gitState.files = gitState.files.filter((f) => !f.conflicted);
  },
  git_continue: () => {
    gitState.merging = false;
    gitState.commits.unshift({ subject: "Merge 'main' into feature/refunds", author: "Ada Lovelace", ago: 0, merge: true });
  },
  git_resolve: (a) => {
    const p = s(a, "path");
    gitState.files.forEach((f) => {
      if (f.path === p) {
        f.conflicted = false;
        f.staged = true;
        f.unstaged = false;
        f.index = "M";
        f.worktree = ".";
      }
    });
  },
  git_log: (): Commit[] =>
    gitState.commits.map((c, i) => ({
      hash: `${i}`.padStart(40, "a"),
      shortHash: `a${i}c4f2d`.slice(0, 7),
      authorName: c.author,
      authorEmail: `${c.author.split(" ")[0].toLowerCase()}@example.com`,
      date: now() - c.ago,
      subject: c.subject,
      parents: "merge" in c ? ["p1", "p2"] : ["p1"],
      refs: i === 0 ? [`HEAD -> ${gitState.branch}`] : i === 2 ? ["origin/main", "main"] : [],
      ...(i === 1 || i === 2
        ? { signatureStatus: "good" as const, signer: `${c.author} <${c.author.split(" ")[0].toLowerCase()}@payments.example.com>` }
        : i === 4
          ? { signatureStatus: "untrusted" as const, signer: c.author }
          : {}),
    })),
  git_diff: () => SAMPLE_DIFF,
  git_show: () => SAMPLE_DIFF,
  git_review: (a) => ({
    base: s(a, "base"),
    head: s(a, "head"),
    mergeBase: "0f3e1c2",
    ahead: 2,
    behind: 0,
    commits: (handlers.git_log({}) as Commit[]).slice(0, 2),
    files: [
      { path: "api-tests/collections/orders-api/orders/create-order.req.yaml", status: "M", additions: 4, deletions: 1 },
      { path: "api-tests/collections/orders-api/admin/refund-order.req.yaml", status: "A", additions: 12, deletions: 0 },
    ],
    diff: SAMPLE_DIFF,
    conflicts: [],
  }),
  git_compare_url: (a) => `https://github.com/acme/payments-service/compare/${s(a, "base")}...${s(a, "head")}?expand=1`,
  git_remotes: () => [{ name: "origin", url: "git@github.com:acme/payments-service.git", webUrl: "https://github.com/acme/payments-service" }],
  git_add_remote: () => undefined,
  git_set_remote_url: () => undefined,
  git_remove_remote: () => undefined,
  git_identity: () => ({ name: "Ada Lovelace", email: "ada@payments.example.com" }),
  git_set_identity: () => undefined,
  sync_now: async (a) => {
    await delay(500);
    const w = ws(a.workspaceId);
    // The first manual sync of the demo workspace finds teammate commits.
    const report: SyncReport =
      w === db.workspaces[0] && !teammatePushed
        ? { ...syncReport(w), state: "diverged", behind: 2, changedRequests: ["Create order", "Refund order"], authors: ["Grace Hopper"] }
        : syncReport(w);
    if (w === db.workspaces[0]) teammatePushed = true;
    listeners.get("sync-status")?.forEach((h) => h({ workspaceId: a.workspaceId, report }));
    return report;
  },
  get_history: (a) => structuredClone(ws(a.workspaceId).history),
  clear_history: (a) => void (ws(a.workspaceId).history = []),
  restore_history_entry: (a) => {
    const w = ws(a.workspaceId);
    const entry = w.history.find((h) => h.id === a.entryId);
    if (!entry) throw new Error("history entry not found");
    let parent = a.parentId ? findGroup(w, s(a, "parentId"))?.group : w.collections[0];
    if (!parent) {
      parent = group("Requests", [], { seq: 1 });
      w.collections.push(parent);
    }
    const created = req(entry.name || `${entry.method} ${entry.url}`, entry.method, entry.url);
    created.seq = parent.items.length + 1;
    parent.items.push(r(created));
    return structuredClone(created);
  },
  move_item: (a) => {
    const w = ws(a.workspaceId);
    const itemId = s(a, "itemId");
    const targetParentId = (a.targetParentId as string | null) ?? null;
    const beforeId = (a.beforeId as string | null) ?? null;
    if (!targetParentId) {
      const idx = w.collections.findIndex((c) => c.meta.id === itemId);
      if (idx < 0) return;
      const [moved] = w.collections.splice(idx, 1);
      const bIdx = beforeId ? w.collections.findIndex((c) => c.meta.id === beforeId) : -1;
      if (bIdx >= 0) w.collections.splice(bIdx, 0, moved);
      else w.collections.push(moved);
      w.collections.forEach((c, i) => (c.meta.seq = i + 1));
      return;
    }
    const dest = findGroup(w, targetParentId)?.group;
    if (!dest) return;
    let extracted: MockItem | null = null;
    const removeFrom = (g: MockGroup) => {
      const idx = g.items.findIndex((it) => (it.kind === "request" ? it.request.id === itemId : it.group.meta.id === itemId));
      if (idx >= 0) {
        [extracted] = g.items.splice(idx, 1);
        return;
      }
      g.items.forEach((it) => it.kind === "folder" && removeFrom(it.group));
    };
    w.collections.forEach(removeFrom);
    if (!extracted) return;
    const bIdx = beforeId
      ? dest.items.findIndex((it) => (it.kind === "request" ? it.request.id === beforeId : it.group.meta.id === beforeId))
      : -1;
    if (bIdx >= 0) dest.items.splice(bIdx, 0, extracted);
    else dest.items.push(extracted);
    dest.items.forEach((it, i) => {
      if (it.kind === "request") it.request.seq = i + 1;
      else it.group.meta.seq = i + 1;
    });
  },
  oauth_introspect_token: (a) => {
    const o = effectiveOAuth(ws(a.workspaceId), a.request as Request, (a.parentId as string | null) ?? null);
    const token = oauthTokens.get(oauthKey(s(a, "workspaceId"), o));
    if (!token) throw new Error("no cached access token to introspect");
    return {
      active: !token.expiresAt || Date.parse(token.expiresAt) > Date.now(),
      sub: "ada",
      client_id: o.clientId,
      scope: token.scope ?? o.scope ?? "",
      token_type: token.tokenType,
    };
  },
  git_file_changes: (a): FileChanges => {
    const path = s(a, "path");
    const name = path.split("/").pop() ?? path;
    const known = FILE_CHANGES[name];
    return { path, diff: SAMPLE_DIFF, ...(known ?? { kind: "other", additions: 0, deletions: 0, changes: null }) };
  },
  sync_statuses: () => Object.fromEntries(db.workspaces.map((w) => [w.entry.id, syncReport(w)])),

  // Phase 2: templates & onboarding
  list_workspace_templates: (): WorkspaceTemplate[] => structuredClone(MOCK_TEMPLATES),
  create_onboarding_link: (a): string => {
    const w = ws(a.workspaceId);
    const params = new URLSearchParams({ name: w.manifest.name });
    params.set("repo", (a.repoUrl as string | null) || "git@github.com:acme/payments-service.git");
    params.set("branch", (a.branch as string | null) || "main");
    const env = w.environments.find((e) => e.id === w.entry.prefs.activeEnvironment) ?? w.environments[0];
    if (env) {
      params.set("env", env.name);
      const secrets = env.variables.filter((v) => v.secret).map((v) => v.key);
      if (secrets.length) params.set("secrets", secrets.join(","));
    }
    return `courion://onboard?${params.toString()}`;
  },
  parse_onboarding_link: (a): OnboardingSpec => {
    const raw = s(a, "link").trim();
    const q = raw.indexOf("?");
    if (!raw.startsWith("courion://onboard") || q < 0) throw new Error("Not a Courion onboarding link (courion://onboard?…)");
    const p = new URLSearchParams(raw.slice(q + 1));
    const list = (k: string) => (p.get(k) ?? "").split(",").map((x) => x.trim()).filter(Boolean);
    return {
      name: p.get("name") || "Team workspace",
      repoUrl: p.get("repo") ?? undefined,
      branch: p.get("branch") ?? undefined,
      defaultEnv: p.get("env") ?? undefined,
      templateId: p.get("template") ?? undefined,
      sparsePatterns: list("sparse"),
      requiredSecrets: list("secrets"),
    };
  },
  onboard_workspace: (a): WorkspaceEntry => {
    const spec = handlers.parse_onboarding_link({ link: a.link }) as OnboardingSpec;
    const entry = handlers.create_workspace({ path: a.path, name: spec.name, team: null, initGit: false, template: spec.templateId ?? "rest-crud" }) as WorkspaceEntry;
    const w = ws(entry.id);
    const env = w.environments[0];
    if (env) {
      if (spec.defaultEnv) env.name = spec.defaultEnv;
      const secrets = (a.secrets as Record<string, string> | null) ?? {};
      for (const key of spec.requiredSecrets ?? []) env.variables.push({ key, value: secrets[key] ?? "", enabled: true, secret: true });
    }
    return structuredClone(w.entry);
  },

  // Phase 2: vault & keychain
  get_vault_settings: (): VaultSettings => structuredClone(mockVault),
  save_vault_settings: (a) => void (mockVault = structuredClone(a.settings as VaultSettings)),
  vault_providers_status: (): VaultProviderStatus[] => [
    { id: "os-keychain", name: "OS keychain", scheme: "keychain://", available: true, detail: mockVault.useOsKeychain ? "Environment secrets are stored in the OS keychain" : "Off: secrets stay in .trax/state.json" },
    { id: "1password", name: "1Password", scheme: "op://vault/item/field", available: true, detail: mockVault.onepasswordConnectUrl ? "Configured via 1Password Connect API" : "1Password CLI (`op`) detected" },
    { id: "hashicorp-vault", name: "HashiCorp Vault", scheme: "vault://mount/path#field", available: !!mockVault.hashicorpAddr, detail: mockVault.hashicorpAddr ? `Configured (${mockVault.hashicorpAddr})` : "Set HashiCorp Vault URL & token below or VAULT_ADDR" },
    { id: "azure-keyvault", name: "Azure Key Vault", scheme: "azkv://vault-name/secret-name", available: false, detail: "Set Azure Key Vault token below or sign in with `az login`" },
  ],
  resolve_secret_reference: (a): string => {
    const v = s(a, "value").trim();
    if (!/^(op|vault|azkv|keychain):\/\//.test(v)) return v;
    if (v.startsWith("azkv://")) throw new Error("Azure Key Vault token not set and `az` CLI failed");
    return `demo-${v.split(/[/#]/).pop()}`;
  },

  // Phase 2: pull requests & inline review comments
  get_pr_settings: (): PrContext => ({
    settings: structuredClone(mockPrSettings),
    hosting: { provider: "github", host: "github.com", apiBase: "https://api.github.com", webBase: "https://github.com", owner: "acme", repo: "payments-service", fullPath: "acme/payments-service" },
  }),
  save_pr_settings: (a) => void (mockPrSettings = structuredClone(a.settings as PrSettings)),
  list_pull_requests: (a): PullRequest[] => {
    const filter = (a.stateFilter as string | null) || "open";
    return structuredClone(filter === "all" ? mockPulls : mockPulls.filter((p) => p.state === filter));
  },
  create_pull_request: (a): PullRequest => {
    const input = a.input as CreatePullRequestInput;
    if (mockPulls.some((p) => p.state === "open" && p.sourceBranch === input.sourceBranch && p.targetBranch === input.targetBranch)) {
      throw new Error(`A pull request for ${input.sourceBranch} already exists`);
    }
    const number = Math.max(40, ...mockPulls.map((p) => p.number)) + 1;
    const at = new Date().toISOString();
    const pr: PullRequest = {
      number,
      title: input.title,
      body: input.body,
      state: "open",
      draft: input.draft,
      author: "ada",
      sourceBranch: input.sourceBranch,
      targetBranch: input.targetBranch,
      webUrl: `https://github.com/acme/payments-service/pull/${number}`,
      createdAt: at,
      updatedAt: at,
      approved: false,
      commentCount: 0,
      provider: "github",
    };
    mockPulls.unshift(pr);
    return structuredClone(pr);
  },
  approve_pull_request: (a): PullRequest => {
    const pr = mockPulls.find((p) => p.number === Number(a.prNumber));
    if (!pr) throw new Error(`pull request #${a.prNumber} not found`);
    pr.approved = true;
    return structuredClone(pr);
  },
  merge_pull_request: (a): PullRequest => {
    const pr = mockPulls.find((p) => p.number === Number(a.prNumber));
    if (!pr) throw new Error(`pull request #${a.prNumber} not found`);
    pr.state = "merged";
    pr.updatedAt = new Date().toISOString();
    return structuredClone(pr);
  },
  list_review_comments: (a): ReviewComment[] => {
    const pr = a.prNumber as number | null | undefined;
    const requestId = a.requestId as string | null | undefined;
    return structuredClone(
      mockComments.filter((c) => (pr == null || c.prNumber === pr) && (!requestId || c.requestId === requestId)),
    );
  },
  add_review_comment: (a): ReviewComment => {
    const input = a.input as AddReviewCommentInput;
    const w = ws(a.workspaceId);
    const found = input.requestId ? findRequest(w, input.requestId) : null;
    const comment: ReviewComment = {
      id: uid(),
      prNumber: input.prNumber ?? undefined,
      requestId: input.requestId ?? undefined,
      requestName: found?.request.name,
      path: input.path || "collections/orders-api/orders/create-order.req.yaml",
      field: input.field || "general",
      line: input.line ?? 1,
      body: input.body,
      author: "Ada Lovelace",
      createdAt: new Date().toISOString(),
      resolved: false,
      source: input.prNumber ? "github" : "local",
    };
    mockComments.push(comment);
    const pr = mockPulls.find((p) => p.number === input.prNumber);
    if (pr) pr.commentCount += 1;
    return structuredClone(comment);
  },
  resolve_review_comment: (a): ReviewComment => {
    const c = mockComments.find((x) => x.id === a.commentId);
    if (!c) throw new Error("review comment not found");
    c.resolved = !!a.resolved;
    return structuredClone(c);
  },

  // Phase 2: semantic conflict preview & resolution
  git_conflict_preview: (a): ConflictPreview => {
    const choices = (a.choices as Record<string, MergeSide> | undefined) ?? {};
    const chosen = choices["headers.Authorization"] ?? "ours";
    const value = { base: "Bearer {{token}}", ours: "Bearer {{adminToken}}", theirs: "Bearer {{serviceToken}}" }[chosen];
    return {
      stages: { base: CONFLICT_BASE, ours: CONFLICT_OURS, theirs: CONFLICT_THEIRS },
      semantic: {
        path: s(a, "path"),
        kind: "request",
        canAutoMerge: false,
        autoMerged: ["Header `X-Client` (ours)", "Query param `dryRun` (theirs)"],
        conflicts: [
          {
            id: "headers.Authorization",
            section: "Headers",
            field: "Authorization",
            label: "Header `Authorization`",
            base: "Bearer {{token}}",
            ours: "Bearer {{adminToken}}",
            theirs: "Bearer {{serviceToken}}",
            chosen,
          },
        ],
        mergedYaml: `id: req-create-order\nname: Create order\nmethod: POST\nurl: '{{baseUrl}}/v1/orders'\nparams:\n- key: dryRun\n  value: 'true'\nheaders:\n- key: Authorization\n  value: ${value}\n- key: X-Client\n  value: web\n`,
      },
    };
  },
  git_resolve_semantic: (a) => handlers.git_resolve({ path: a.path }),

  // Phase 2: stashes, cherry-pick, revert, tags, sparse checkout, signing
  git_stashes: (): StashEntry[] => structuredClone(mockStashes),
  git_stash_push: (a) => {
    const changed = gitState.files.filter((f) => !f.untracked || a.includeUntracked);
    if (changed.length === 0) throw new Error("No local changes to save");
    gitState.files = gitState.files.filter((f) => !changed.includes(f));
    mockStashes.unshift({ index: 0, reference: "stash@{0}", message: `On ${gitState.branch}: ${(a.message as string | null) || "WIP"}`, date: now() });
    renumberStashes();
  },
  git_stash_apply: (a): MergeOutcome => {
    if (a.pop) {
      mockStashes.splice(Number(a.index), 1);
      renumberStashes();
    }
    return { kind: "merged" };
  },
  git_stash_drop: (a) => {
    mockStashes.splice(Number(a.index), 1);
    renumberStashes();
  },
  git_cherry_pick: (a): MergeOutcome => {
    gitState.commits.unshift({ subject: `Cherry-pick ${s(a, "commit").slice(0, 7)}`, author: "Ada Lovelace", ago: 0 });
    return { kind: "merged" };
  },
  git_revert: (a): MergeOutcome => {
    const target = (handlers.git_log({}) as Commit[]).find((c) => c.hash === a.commit);
    gitState.commits.unshift({ subject: `Revert "${target?.subject ?? s(a, "commit").slice(0, 7)}"`, author: "Ada Lovelace", ago: 0 });
    return { kind: "merged" };
  },
  git_tags: (): Tag[] => structuredClone(mockTags),
  git_create_tag: (a) => {
    const name = s(a, "name");
    if (mockTags.some((t) => t.name === name)) throw new Error(`tag '${name}' already exists`);
    const hash = (a.target as string | null) || "a".repeat(40);
    mockTags.unshift({ name, hash, shortHash: hash.slice(0, 7), message: (a.message as string | null) || null, date: now() });
  },
  git_delete_tag: (a) => void (mockTags = mockTags.filter((t) => t.name !== a.name)),
  git_push_tag: () => delay(300),
  git_sparse_checkout_status: (): SparseCheckoutStatus => structuredClone(mockSparse),
  git_sparse_checkout_set: (a) => void (mockSparse = { enabled: true, cone: a.cone !== false, patterns: (a.patterns as string[]).filter(Boolean) }),
  git_sparse_checkout_disable: () => void (mockSparse = { enabled: false, cone: true, patterns: [] }),
  git_signing_settings: (): SigningSettings => structuredClone(mockSigning),
  git_set_signing_settings: (a) => void (mockSigning = structuredClone(a.settings as SigningSettings)),
};

const MOCK_TEMPLATES: WorkspaceTemplate[] = [
  { id: "blank", name: "Blank workspace", description: "One collection with a health check and a Development environment.", tags: ["starter"], environments: ["Development"], requestCount: 1 },
  { id: "rest-crud", name: "REST CRUD API", description: "List, create, read, update and delete with tests, plus Local and Staging environments.", tags: ["rest"], environments: ["Local", "Staging"], requestCount: 5 },
  { id: "graphql-service", name: "GraphQL service", description: "Queries and a mutation with variables and response assertions.", tags: ["graphql"], environments: ["Local"], requestCount: 2 },
  { id: "oauth2-microservice", name: "OAuth 2.0 microservice", description: "Client-credentials auth on the collection, protected endpoints inherit it.", tags: ["auth"], environments: ["Local", "Staging"], requestCount: 2 },
  { id: "webhook-suite", name: "Webhook test suite", description: "Signed webhook deliveries with idempotency keys and replay checks.", tags: ["events"], environments: ["Local"], requestCount: 2 },
];

const CONFLICT_BASE = "id: req-create-order\nname: Create order\nmethod: POST\nurl: '{{baseUrl}}/v1/orders'\nheaders:\n- key: Authorization\n  value: Bearer {{token}}\n";
const CONFLICT_OURS = "id: req-create-order\nname: Create order\nmethod: POST\nurl: '{{baseUrl}}/v1/orders'\nheaders:\n- key: Authorization\n  value: Bearer {{adminToken}}\n- key: X-Client\n  value: web\n";
const CONFLICT_THEIRS = "id: req-create-order\nname: Create order\nmethod: POST\nurl: '{{baseUrl}}/v1/orders'\nparams:\n- key: dryRun\n  value: 'true'\nheaders:\n- key: Authorization\n  value: Bearer {{serviceToken}}\n";

let mockVault: VaultSettings = { useOsKeychain: true };
let mockPrSettings: PrSettings = {};

const mockPulls: PullRequest[] = [
  {
    number: 41,
    title: "Add refund flow requests and idempotency checks",
    body: "Adds `POST /v1/orders/{{orderId}}/refund` and asserts the Idempotency-Key replay.",
    state: "open",
    draft: false,
    author: "grace",
    sourceBranch: "feature/refunds",
    targetBranch: "main",
    webUrl: "https://github.com/acme/payments-service/pull/41",
    createdAt: new Date(Date.now() - 5 * 3600_000).toISOString(),
    updatedAt: new Date(Date.now() - 1800_000).toISOString(),
    approved: false,
    commentCount: 1,
    provider: "github",
  },
];

const mockComments: ReviewComment[] = [
  {
    id: "rc-1",
    prNumber: 41,
    path: "collections/orders-api/orders/create-order.req.yaml",
    field: "headers.Idempotency-Key",
    line: 8,
    requestName: "Create order",
    body: "Can we also assert that replaying the same Idempotency-Key returns the first response?",
    author: "Grace Hopper",
    createdAt: new Date(Date.now() - 3600_000).toISOString(),
    resolved: false,
    source: "github",
  },
];

const mockStashes: StashEntry[] = [{ index: 0, reference: "stash@{0}", message: "On feature/refunds: webhook retry headers", date: now() - 7200 }];
const renumberStashes = () =>
  mockStashes.forEach((st, i) => {
    st.index = i;
    st.reference = `stash@{${i}}`;
  });

let mockTags: Tag[] = [{ name: "v1.2.0", hash: `${2}`.padStart(40, "a"), shortHash: "aaaaaaa", message: "Collections for the 1.2 release", date: now() - 3 * 86400 }];
let mockSparse: SparseCheckoutStatus = { enabled: false, cone: true, patterns: [] };
let mockSigning: SigningSettings = { enabled: false, format: "ssh", signingKey: null, program: null };

export async function mockInvoke<T>(cmd: string, args: Args): Promise<T> {
  const handler = handlers[cmd];
  if (!handler) throw new Error(`mock backend: ${cmd} is not implemented`);
  await delay(15);
  try {
    return structuredClone(await handler(args)) as T;
  } catch (e) {
    throw e instanceof Error ? e.message : e;
  }
}
