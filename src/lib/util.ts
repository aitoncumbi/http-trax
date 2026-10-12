import type { GroupNode, Request, TreeItem } from "./types";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] as const;

/** Wheel handler for a sideways-scrolling tab strip: a vertical wheel scrolls it too. */
export function scrollSideways(e: { currentTarget: HTMLElement; deltaX: number; deltaY: number }): void {
  const el = e.currentTarget;
  if (el.scrollWidth > el.clientWidth && Math.abs(e.deltaY) > Math.abs(e.deltaX)) el.scrollLeft += e.deltaY;
}

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

/** Short label used in method pills (DELETE → DEL, OPTIONS → OPT). */
export function methodLabel(method: string): string {
  const m = method.toUpperCase();
  if (m === "DELETE") return "DEL";
  if (m === "OPTIONS") return "OPT";
  return m.length > 5 ? m.slice(0, 5) : m;
}

/** Tailwind classes for a method pill (text + 14% tinted background). */
export function methodPill(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-method-get bg-method-get/14";
    case "POST":
      return "text-method-post bg-method-post/14";
    case "PUT":
      return "text-method-put bg-method-put/14";
    case "PATCH":
      return "text-method-patch bg-method-patch/14";
    case "DELETE":
      return "text-method-delete bg-method-delete/14";
    default:
      return "text-muted bg-selected";
  }
}

/** Colour cue for an environment by its name: production red, staging amber, local green. */
export function envTone(name: string | null | undefined): string {
  const n = (name ?? "").toLowerCase();
  if (!n) return "var(--dim)";
  if (/\bprod|live\b/.test(n)) return "var(--danger)";
  if (/stag|uat|pre|qa|test|sandbox/.test(n)) return "var(--warn)";
  if (/local|dev/.test(n)) return "var(--ok)";
  return "var(--info)";
}

export function initials(name: string): string {
  // "Northwind (client)" → NW: parenthetical notes aren't part of the name.
  const words = name
    .replace(/\([^)]*\)/g, " ")
    .replace(/[^\p{L}\p{N}\s]/gu, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  if (words.length >= 2) return (words[0][0] + words[1][0]).toUpperCase();
  return (words[0] ?? "?").slice(0, 2).toUpperCase();
}

const AVATAR_TONES = [
  { bg: "rgba(94,106,210,0.22)", fg: "var(--accent-text-2)" },
  { bg: "rgba(195,155,255,0.16)", fg: "var(--var)" },
  { bg: "rgba(95,211,154,0.16)", fg: "var(--ok)" },
  { bg: "rgba(240,180,92,0.16)", fg: "var(--warn)" },
  { bg: "rgba(255,123,114,0.16)", fg: "var(--danger)" },
];

/** Stable avatar colours per workspace id. */
export function avatarTone(id: string): { bg: string; fg: string } {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return AVATAR_TONES[h % AVATAR_TONES.length];
}
