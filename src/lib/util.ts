import type { GroupNode, Request, TreeItem } from "./types";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] as const;

/** Text colour class for an HTTP method badge. */
export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-method-get";
    case "POST":
      return "text-method-post";
    case "PUT":
      return "text-method-put";
    case "PATCH":
      return "text-method-patch";
    case "DELETE":
      return "text-method-delete";
    default:
      return "text-muted";
  }
}

export function statusColor(status: number): string {
  if (status >= 500) return "text-danger";
  if (status >= 400) return "text-warn";
  if (status >= 300) return "text-info";
  return "text-ok";
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(2)} s`;
}

export function timeAgo(input: string | number | null | undefined): string {
  if (input === null || input === undefined) return "never";
  const date = typeof input === "number" ? new Date(input * 1000) : new Date(input);
  const secs = Math.round((Date.now() - date.getTime()) / 1000);
  if (secs < 5) return "just now";
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}d ago`;
  return date.toLocaleDateString();
}

export function uid(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return Math.random().toString(36).slice(2) + Date.now().toString(36);
}

export function newRequest(name = "New request"): Request {
  return {
    id: "",
    name,
    seq: 0,
    method: "GET",
    url: "",
    params: [],
    headers: [],
    body: { mode: "none" },
    auth: { type: "inherit" },
    scripts: { preRequest: "", postResponse: "" },
    settings: { followRedirects: true, verifyTls: true },
  };
}

/** Fill optional fields so editors can bind to them directly. */
export function normaliseRequest(r: Request): Request {
  return {
    ...r,
    description: r.description ?? "",
    params: r.params ?? [],
    headers: r.headers ?? [],
    body: r.body ?? { mode: "none" },
    auth: r.auth ?? { type: "inherit" },
    scripts: { preRequest: r.scripts?.preRequest ?? "", postResponse: r.scripts?.postResponse ?? "" },
    settings: r.settings ?? { followRedirects: true, verifyTls: true },
  };
}

/** Find the chain of groups (collection first) that contains `id` (a group or request). */
export function findChain(collections: GroupNode[], id: string): GroupNode[] | null {
  const walk = (node: GroupNode, chain: GroupNode[]): GroupNode[] | null => {
    const here = [...chain, node];
    if (node.meta.id === id) return here;
    for (const item of node.items) {
      if (item.kind === "request" && item.id === id) return here;
      if (item.kind === "folder") {
        const found = walk(item, here);
        if (found) return found;
      }
    }
    return null;
  };
  for (const c of collections) {
    const found = walk(c, []);
    if (found) return found;
  }
  return null;
}

export function countRequests(items: TreeItem[]): number {
  return items.reduce((n, i) => n + (i.kind === "request" ? 1 : countRequests(i.items)), 0);
}

/** Does this node (or anything inside it) match the filter text? */
export function filterTree(items: TreeItem[], query: string): TreeItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  const out: TreeItem[] = [];
  for (const item of items) {
    if (item.kind === "request") {
      if (item.name.toLowerCase().includes(q) || item.url.toLowerCase().includes(q) || item.method.toLowerCase() === q) {
        out.push(item);
      }
    } else {
      const children = item.meta.name.toLowerCase().includes(q) ? item.items : filterTree(item.items, q);
      if (children.length || item.meta.name.toLowerCase().includes(q)) out.push({ ...item, items: children });
    }
  }
  return out;
}

export function prettyJson(text: string): string | null {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return null;
  }
}

export function stableStringify(value: unknown): string {
  return JSON.stringify(value);
}
