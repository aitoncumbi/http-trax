// Cookie helpers for the response Cookies tab and the cookie manager.

import type { CookieInfo, CookieKey } from "./types";

export const cookieKey = (c: CookieKey): string => `${c.domain}\u0000${c.path}\u0000${c.name}`;

function hostAndPath(url: string): { host: string; path: string } {
  try {
    const u = new URL(url.includes("://") ? url : `http://${url}`);
    return { host: u.hostname.toLowerCase(), path: u.pathname || "/" };
  } catch {
    return { host: "", path: "/" };
  }
}

/** Host of a URL (scheme optional), or null when it isn't one yet. */
export function hostOf(url: string): string | null {
  if (!url.trim() || url.includes("{{")) return null;
  return hostAndPath(url).host || null;
}

/** RFC 6265 default-path: the request path up to its last "/". */
function defaultPath(path: string): string {
  if (!path.startsWith("/")) return "/";
  const at = path.lastIndexOf("/");
  return at <= 0 ? "/" : path.slice(0, at);
}

const isoSeconds = (ms: number) => new Date(ms).toISOString().replace(/\.\d+Z$/, "Z");

/** A `Set-Cookie` header as the jar would store it for `url`. */
export function parseSetCookie(header: string, url: string, now = Date.now()): CookieInfo | null {
  const [pair, ...attrs] = header.split(";");
  const eq = pair.indexOf("=");
  if (eq <= 0) return null;
  const { host, path } = hostAndPath(url);
  const cookie: CookieInfo = {
    name: pair.slice(0, eq).trim(),
    value: pair.slice(eq + 1).trim().replace(/^"(.*)"$/, "$1"),
    domain: host,
    includeSubdomains: false,
    path: defaultPath(path),
    expires: null,
    secure: false,
    httpOnly: false,
    sameSite: null,
  };
  let maxAge: number | null = null;
  for (const attr of attrs) {
    const i = attr.indexOf("=");
    const key = (i < 0 ? attr : attr.slice(0, i)).trim().toLowerCase();
    const val = i < 0 ? "" : attr.slice(i + 1).trim();
    switch (key) {
      case "domain":
        if (val) {
          cookie.domain = val.replace(/^\./, "").toLowerCase();
          cookie.includeSubdomains = true;
        }
        break;
      case "path":
        if (val.startsWith("/")) cookie.path = val;
        break;
      case "expires": {
        const t = Date.parse(val);
        if (!Number.isNaN(t)) cookie.expires = isoSeconds(t);
        break;
      }
      case "max-age":
        if (/^-?\d+$/.test(val)) maxAge = Number(val);
        break;
      case "secure":
        cookie.secure = true;
        break;
      case "httponly":
        cookie.httpOnly = true;
        break;
      case "samesite":
        cookie.sameSite = val ? val[0].toUpperCase() + val.slice(1).toLowerCase() : null;
        break;
    }
  }
  // Max-Age wins over Expires.
  if (maxAge !== null) cookie.expires = isoSeconds(now + maxAge * 1000);
  return cookie;
}

/** `a=1; b=2` into pairs. */
export function parseCookieHeader(value: string): { name: string; value: string }[] {
  return value
    .split(";")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => {
      const i = p.indexOf("=");
      return i < 0 ? { name: p, value: "" } : { name: p.slice(0, i), value: p.slice(i + 1) };
    });
}

/** "Session", "Expired", "in 5 min", "in 3 h", "in 29 days", or a date. */
export function expiryLabel(expires: string | null | undefined, now = Date.now()): string {
  if (!expires) return "Session";
  const t = Date.parse(expires);
  if (Number.isNaN(t)) return expires;
  const secs = Math.round((t - now) / 1000);
  if (secs <= 0) return "Expired";
  if (secs < 3600) return `in ${Math.max(1, Math.round(secs / 60))} min`;
  if (secs < 86_400) return `in ${Math.round(secs / 3600)} h`;
  const days = Math.round(secs / 86_400);
  if (days < 365) return `in ${days} day${days === 1 ? "" : "s"}`;
  return new Date(t).toLocaleDateString();
}

/** `YYYY-MM-DDTHH:mm` in local time, for `<input type="datetime-local">`. */
export function toLocalInput(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fromLocalInput(local: string): string | null {
  if (!local) return null;
  const t = new Date(local).getTime();
  return Number.isNaN(t) ? null : isoSeconds(t);
}
