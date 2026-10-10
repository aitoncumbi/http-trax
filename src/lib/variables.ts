// Which `{{variables}}` a request can use, where each value comes from, and
// helpers for completing `{{` in text fields. Mirrors the runner's precedence:
// runtime > environment (local values over shared) > collection > workspace.

import type { EnvironmentView, Variable } from "./types";

export type VarSource = "workspace" | "collection" | "environment" | "local" | "runtime" | "dynamic";

export interface VarInfo {
  name: string;
  value: string;
  source: VarSource;
  /** "Local environment", "Orders API collection", ... */
  label: string;
  secret?: boolean;
}

export type VarScope = Map<string, VarInfo>;

export const DYNAMIC_VARS: { name: string; description: string }[] = [
  { name: "$uuid", description: "A new UUID v4 for every send" },
  { name: "$guid", description: "Same as $uuid" },
  { name: "$randomUUID", description: "Same as $uuid" },
  { name: "$timestamp", description: "Unix time in seconds" },
  { name: "$timestampMs", description: "Unix time in milliseconds" },
  { name: "$isoTimestamp", description: "Current time, ISO 8601" },
  { name: "$randomInt", description: "A random integer from 0 to 1000" },
];

export function buildScope(input: {
  workspace?: Variable[];
  collection?: { name: string; variables?: Variable[] } | null;
  environment?: EnvironmentView | null;
  runtime?: Record<string, string>;
}): VarScope {
  const scope: VarScope = new Map();
  for (const d of DYNAMIC_VARS) scope.set(d.name, { name: d.name, value: d.description, source: "dynamic", label: "Generated on send" });
  const add = (vars: Variable[] | undefined, source: VarSource, label: string) => {
    for (const v of vars ?? []) {
      if (v.enabled === false || !v.key) continue;
      scope.set(v.key, { name: v.key, value: v.value, source, label, secret: v.secret });
    }
  };
  add(input.workspace, "workspace", "Workspace");
  if (input.collection) add(input.collection.variables, "collection", `${input.collection.name} collection`);
  const env = input.environment;
  if (env) {
    add(env.variables, "environment", `${env.name} environment`);
    for (const [k, v] of Object.entries(env.overrides ?? {})) {
      const secret = env.variables.find((x) => x.key === k)?.secret;
      scope.set(k, { name: k, value: v, source: "local", label: `${env.name}, set by a script on this device`, secret });
    }
  }
  for (const [k, v] of Object.entries(input.runtime ?? {})) {
    scope.set(k, { name: k, value: v, source: "runtime", label: "Session variable (trax.vars.set)" });
  }
  return scope;
}

/** `{{name}}` tokens in a string, with positions. */
export function findVars(text: string): { name: string; start: number; end: number }[] {
  const out: { name: string; start: number; end: number }[] = [];
  const re = /\{\{([^{}]*)\}\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) out.push({ name: m[1].trim(), start: m.index, end: m.index + m[0].length });
  return out;
}

/** An unfinished `{{name` right before the caret: where it starts and what was typed. */
export function openVarAt(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const m = /\{\{\s*([\w.$-]*)$/.exec(before);
  return m ? { start: m.index, query: m[1] } : null;
}

/** Replace the unfinished `{{query` with `{{name}}`, reusing closing braces already there. */
export function completeVar(text: string, caret: number, name: string): { text: string; caret: number } | null {
  const open = openVarAt(text, caret);
  if (!open) return null;
  let rest = text.slice(caret);
  // Drop the rest of a half-typed name and an existing "}}".
  rest = rest.replace(/^[\w.$-]*\s*(\}\})?/, "");
  const inserted = `{{${name}}}`;
  return { text: text.slice(0, open.start) + inserted + rest, caret: open.start + inserted.length };
}

/** Variables matching what was typed: prefix matches first, then substring. */
export function suggest(scope: VarScope, query: string, limit = 8): VarInfo[] {
  const q = query.toLowerCase();
  const all = [...scope.values()];
  const rank = (v: VarInfo) => {
    const n = v.name.toLowerCase();
    if (!q) return v.source === "dynamic" ? 2 : 0;
    if (n.startsWith(q)) return 0;
    if (n.includes(q)) return 1;
    return -1;
  };
  return all
    .map((v) => ({ v, r: rank(v) }))
    .filter((x) => x.r >= 0)
    .sort((a, b) => a.r - b.r || a.v.name.localeCompare(b.v.name))
    .slice(0, limit)
    .map((x) => x.v);
}

/** Text with variables filled in from the scope (dynamic ones stay as placeholders). */
export function resolveText(text: string, scope: VarScope, depth = 0): string {
  return text.replace(/\{\{([^{}]*)\}\}/g, (m, raw: string) => {
    const v = scope.get(raw.trim());
    if (!v || v.source === "dynamic") return m;
    return depth < 8 ? resolveText(v.value, scope, depth + 1) : v.value;
  });
}
