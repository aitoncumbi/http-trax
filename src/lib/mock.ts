// In-memory stand-in for the Rust backend, used when the UI runs in a plain
// browser (`npm run dev`). It exists so the interface can be developed and
// previewed without Tauri; it doesn't send real HTTP requests or run git.

import type {
  Branch,
  Commit,
  Environment,
  EnvironmentView,
  GroupMeta,
  GroupNode,
  RepoStatus,
  Request,
  RunResult,
  SavedResponse,
  SyncReport,
  TreeItem,
  WorkspaceEntry,
  WorkspaceManifest,
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
}

const uid = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2) + Date.now().toString(36);

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
  const orders = group(
    "Orders API",
    [
      r(
        req("List orders", "GET", "{{baseUrl}}/v1/orders", {
          params: [kv("status", "open"), { key: "limit", value: "20", enabled: false }],
          scripts: {
            postResponse:
              "trax.test(\"status is 200\", () => {\n  trax.expect(trax.response.status).toBe(200);\n});\n\ntrax.test(\"returns a list\", () => {\n  trax.expect(trax.response.json().items).toBeTypeOf(\"array\");\n});\n",
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
              "trax.test(\"order created\", () => {\n  trax.expect(trax.response.status).toBe(201);\n});\n\n// Chain the next request\ntrax.env.set(\"orderId\", trax.response.json().id);\n",
          },
        }),
      ),
      r(req("Get order", "GET", "{{baseUrl}}/v1/orders/{{orderId}}")),
      f(
        group("Admin", [
          r(req("Refund order", "POST", "{{baseUrl}}/v1/orders/{{orderId}}/refund", { body: { mode: "json", text: '{\n  "reason": "requested_by_customer"\n}' } })),
          r(req("Delete order", "DELETE", "{{baseUrl}}/v1/orders/{{orderId}}")),
        ]),
      ),
    ],
    {
      variables: [{ key: "apiVersion", value: "v1", enabled: true }],
      headers: [kv("Accept", "application/json")],
      auth: { type: "bearer", token: "{{token}}" },
      scripts: { preRequest: "// Runs before every request in this collection\ntrax.request.headers.set(\"X-Request-Id\", trax.uuid());\n" },
    },
  );
  const customers = group("Customers API", [
    r(req("Search customers", "GET", "{{baseUrl}}/v1/customers", { params: [kv("q", "ada")] })),
    r(req("Create customer", "POST", "{{baseUrl}}/v1/customers", { body: { mode: "json", text: '{\n  "name": "Ada Lovelace",\n  "email": "ada@example.com"\n}' } })),
  ]);
  const auth = group("Auth", [
    r(
      req("Login", "POST", "{{authUrl}}/oauth/token", {
        body: { mode: "form-urlencoded", fields: [kv("grant_type", "client_credentials"), kv("client_id", "{{clientId}}"), kv("client_secret", "{{clientSecret}}")] },
        scripts: { postResponse: "trax.env.set(\"token\", trax.response.json().access_token);\n" },
      }),
    ),
  ]);
  const webhooks = group("Webhooks", [r(req("Stripe webhook", "POST", "{{baseUrl}}/webhooks/stripe"))]);
  [orders, customers, auth, webhooks].forEach((c, i) => (c.meta.seq = i + 1));

  const local: EnvironmentView = {
    id: uid(),
    name: "Local",
    variables: [
      { key: "baseUrl", value: "http://localhost:8080", enabled: true },
      { key: "authUrl", value: "http://localhost:9000", enabled: true },
      { key: "clientId", value: "payments-dev", enabled: true },
      { key: "clientSecret", value: "dev-secret", enabled: true, secret: true },
    ],
    overrides: { token: "eyJhbGciOi…", orderId: "ord_42" },
  };
  const staging: EnvironmentView = {
    id: uid(),
    name: "Staging",
    variables: [
      { key: "baseUrl", value: "https://staging.payments.example.com", enabled: true },
      { key: "authUrl", value: "https://auth.staging.example.com", enabled: true },
    ],
    overrides: {},
  };

  const payments: MockWorkspace = {
    entry: {
      id: uid(),
      name: "Payments",
      path: "/home/dev/payments-service/api-tests",
      team: "Payments Team",
      sync: { enabled: true, intervalSecs: 60, autoPull: true, remote: "origin" },
      prefs: { bookmarks: [orders.meta.id], scopedCollection: null, activeEnvironment: local.id },
    },
    manifest: { version: 1, id: "", name: "Payments", team: "Payments Team" },
    collections: [orders, customers, auth, webhooks],
    environments: [local, staging],
    saved: {},
    runtime: { traceId: "t-91c2" },
  };
  payments.manifest.id = payments.entry.id;

  const platform: MockWorkspace = {
    entry: {
      id: uid(),
      name: "Platform",
      path: "/home/dev/platform/collections",
      team: "Platform",
      sync: { enabled: true, intervalSecs: 60, autoPull: true, remote: "origin" },
      prefs: { bookmarks: [], scopedCollection: null, activeEnvironment: null },
    },
    manifest: { version: 1, id: "", name: "Platform", team: "Platform" },
    collections: [group("Feature flags", [r(req("List flags", "GET", "{{baseUrl}}/flags"))], { seq: 1 })],
    environments: [],
    saved: {},
    runtime: {},
  };
  platform.manifest.id = platform.entry.id;
  return [payments, platform];
}

const db = {
  workspaces: seed(),
  active: null as string | null,
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
  const scripts = [...chain.map((g) => g.meta.scripts?.postResponse ?? ""), request.scripts?.postResponse ?? ""];
  const tests = scripts.flatMap((s, i) =>
    [...s.matchAll(/(?:trax|pm)\.test\(\s*["'`](.+?)["'`]/g)].map((m) => ({
      name: m[1],
      passed: true,
      source: i < chain.length ? (i === 0 ? `collection:${chain[0].meta.name}` : `folder:${chain[i].meta.name}`) : "request",
    })),
  );
  const responseBody =
    status === 204
      ? ""
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
        { key: "content-type", value: "application/json" },
        { key: "x-request-id", value: uid() },
        { key: "date", value: new Date().toUTCString() },
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

const gitState = {
  branch: "feature/refunds",
  files: [
    { path: "api-tests/collections/orders-api/create-order.req.yaml", index: ".", worktree: "M", staged: false, unstaged: true, untracked: false, conflicted: false, inWorkspace: true },
    { path: "api-tests/collections/orders-api/admin/refund-order.req.yaml", index: "?", worktree: "?", staged: false, unstaged: true, untracked: true, conflicted: false, inWorkspace: true },
    { path: "api-tests/environments/local.env.yaml", index: "M", worktree: ".", staged: true, unstaged: false, untracked: false, conflicted: false, inWorkspace: true },
  ],
  commits: [
    { subject: "Add refund flow requests", author: "Ada Lovelace", ago: 3600 },
    { subject: "Assert idempotency on create order", author: "Grace Hopper", ago: 7200 },
    { subject: "Merge branch 'feature/customers' into main", author: "Linus T.", ago: 86400, merge: true },
    { subject: "Add customers collection", author: "Ada Lovelace", ago: 90000 },
    { subject: "Initial workspace", author: "Ada Lovelace", ago: 200000 },
  ],
};

const SAMPLE_DIFF = `diff --git a/api-tests/collections/orders-api/create-order.req.yaml b/api-tests/collections/orders-api/create-order.req.yaml
index 3b18e51..a9c4f2d 100644
--- a/api-tests/collections/orders-api/create-order.req.yaml
+++ b/api-tests/collections/orders-api/create-order.req.yaml
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

type Args = Record<string, unknown>;
const s = (a: Args, k: string) => a[k] as string;

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
    const w: MockWorkspace = {
      entry: { id, name: s(a, "name"), path: s(a, "path"), team: (a.team as string) || null, sync: { enabled: true, intervalSecs: 60, autoPull: true, remote: "origin" }, prefs: { bookmarks: [] } },
      manifest: { version: 1, id, name: s(a, "name"), team: (a.team as string) || undefined },
      collections: [],
      environments: [],
      saved: {},
      runtime: {},
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
    else w.environments.push({ ...env, overrides: {} });
    return env;
  },
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
    return fakeRun(w, request, chain, (a.environmentId as string) ?? null);
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
      merging: false,
      rebasing: false,
      conflicts: 0,
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
    return { hash: uid(), shortHash: uid().slice(0, 7), authorName: "You", authorEmail: "you@example.com", date: now(), subject: s(a, "message"), parents: [], refs: [] };
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
  git_merge: async () => {
    await delay(300);
    return { kind: "merged" };
  },
  git_abort: () => undefined,
  git_continue: () => undefined,
  git_resolve: () => undefined,
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
      { path: "api-tests/collections/orders-api/create-order.req.yaml", status: "M", additions: 4, deletions: 1 },
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
    const report = syncReport(ws(a.workspaceId));
    listeners.get("sync-status")?.forEach((h) => h({ workspaceId: a.workspaceId, report }));
    return report;
  },
  sync_statuses: () => Object.fromEntries(db.workspaces.map((w) => [w.entry.id, syncReport(w)])),
};

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
