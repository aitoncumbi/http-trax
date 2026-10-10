import type { SyncReport } from "./types";

/** Helpers for team review: comment fields and teammate change summaries. */

/** Request fields a comment can point at, from the keys in a file's diff. */
export function fieldOptions(diff: string): string[] {
  const out = new Set(["general", "url", "method", "body", "auth", "scripts.preRequest", "scripts.postResponse"]);
  let section: string | null = null;
  for (const line of diff.split("\n")) {
    const text = line.slice(1);
    const top = text.match(/^(headers|params):/);
    if (top) section = top[1];
    // Any other top-level key ends the section; `- key:` items belong to it.
    else if (/^[^\s-]/.test(text)) section = null;
    const key = text.match(/^-\s+key:\s*['"]?([^'"]+)['"]?\s*$/);
    if (section && key) out.add(`${section}.${key[1]}`);
  }
  return [...out];
}

/** "headers.Authorization" -> "Header Authorization". */
export function fieldLabel(field: string): string {
  const [area, ...rest] = field.split(".");
  const name = rest.join(".");
  if (area === "headers") return `Header ${name}`;
  if (area === "params") return `Param ${name}`;
  if (area === "scripts") return name === "preRequest" ? "Pre-request script" : "Tests";
  if (field === "general") return "General";
  if (field === "url") return "URL";
  return field.charAt(0).toUpperCase() + field.slice(1);
}

/** "Grace changed Create order, List orders and 2 more" from a sync report. */
export function teamChanges(report: SyncReport): string {
  const list = (items: string[], max: number) =>
    items.length <= max ? items.join(", ") : `${items.slice(0, max).join(", ")} and ${items.length - max} more`;
  const who = report.authors?.length ? list(report.authors, 2) : "Your team";
  return `${who} changed ${list(report.changedRequests ?? [], 3)}`;
}
