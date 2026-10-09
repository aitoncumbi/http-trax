// Turn a pasted cURL command into a request.

import type { Auth, Body, KeyValue, Request } from "./types";
import { newRequest } from "./util";

/** Split a shell command line into words (handles quotes, escapes, `\` newlines). */
export function shellWords(input: string): string[] {
  const words: string[] = [];
  let cur = "";
  let has = false;
  let quote: '"' | "'" | null = null;
  const s = input.replace(/\\\r?\n/g, " ").replace(/\^\r?\n/g, " ");
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quote) {
      if (c === quote) quote = null;
      else if (c === "\\" && quote === '"' && i + 1 < s.length && '"\\$`'.includes(s[i + 1])) cur += s[++i];
      else cur += c;
      continue;
    }
    if (c === "'" || c === '"') {
      quote = c;
      has = true;
    } else if (c === "\\" && i + 1 < s.length) {
      cur += s[++i];
      has = true;
    } else if (/\s/.test(c)) {
      if (has) words.push(cur);
      cur = "";
      has = false;
    } else {
      cur += c;
      has = true;
    }
  }
  if (has) words.push(cur);
  return words;
}

const WITH_VALUE = new Set([
  "-X", "--request", "-H", "--header", "-d", "--data", "--data-raw", "--data-binary", "--data-ascii", "--data-urlencode",
  "-u", "--user", "-F", "--form", "--form-string", "-A", "--user-agent", "-b", "--cookie", "-e", "--referer", "--url",
  "-o", "--output", "-m", "--max-time", "--connect-timeout", "-x", "--proxy", "--cert", "--key", "--cacert", "-w", "--write-out",
]);

export interface CurlImport {
  request: Request;
  warnings: string[];
}

export function parseCurl(command: string): CurlImport {
  const words = shellWords(command.trim());
  if (words[0]?.toLowerCase() === "curl" || words[0]?.toLowerCase().endsWith("curl.exe")) words.shift();
  const warnings: string[] = [];
  let method: string | null = null;
  let url = "";
  const headers: KeyValue[] = [];
  const data: string[] = [];
  const form: { key: string; value: string; isFile: boolean }[] = [];
  let urlencode = false;
  let auth: Auth = { type: "inherit" };
  let get = false;
  let insecure = false;

  for (let i = 0; i < words.length; i++) {
    let w = words[i];
    let value: string | undefined;
    if (w.startsWith("--") && w.includes("=")) {
      const eq = w.indexOf("=");
      value = w.slice(eq + 1);
      w = w.slice(0, eq);
    } else if (/^-[A-Za-z]./.test(w) && WITH_VALUE.has(w.slice(0, 2))) {
      value = w.slice(2);
      w = w.slice(0, 2);
    }
    const next = () => value ?? words[++i] ?? "";
    switch (w) {
      case "-X":
      case "--request":
        method = next().toUpperCase();
        break;
      case "-H":
      case "--header": {
        const h = next();
        const colon = h.indexOf(":");
        if (colon > 0) headers.push({ key: h.slice(0, colon).trim(), value: h.slice(colon + 1).trim(), enabled: true });
        break;
      }
      case "-d":
      case "--data":
      case "--data-raw":
      case "--data-binary":
      case "--data-ascii":
        data.push(next());
        break;
      case "--data-urlencode":
        data.push(next());
        urlencode = true;
        break;
      case "-F":
      case "--form":
      case "--form-string": {
        const f = next();
        const eq = f.indexOf("=");
        if (eq > 0) {
          const v = f.slice(eq + 1);
          const isFile = w !== "--form-string" && v.startsWith("@");
          form.push({ key: f.slice(0, eq), value: isFile ? v.slice(1) : v, isFile });
        }
        break;
      }
      case "-u":
      case "--user": {
        const u = next();
        const colon = u.indexOf(":");
        auth = { type: "basic", username: colon >= 0 ? u.slice(0, colon) : u, password: colon >= 0 ? u.slice(colon + 1) : "" };
        break;
      }
      case "-A":
      case "--user-agent":
        headers.push({ key: "User-Agent", value: next(), enabled: true });
        break;
      case "-b":
      case "--cookie":
        headers.push({ key: "Cookie", value: next(), enabled: true });
        break;
      case "-e":
      case "--referer":
        headers.push({ key: "Referer", value: next(), enabled: true });
        break;
      case "--url":
        url = next();
        break;
      case "-G":
      case "--get":
        get = true;
        break;
      case "-k":
      case "--insecure":
        insecure = true;
        break;
      case "-L":
      case "--location":
        // Requests follow redirects by default.
        break;
      case "-I":
      case "--head":
        method = "HEAD";
        break;
      default:
        if (WITH_VALUE.has(w)) {
          next();
          warnings.push(`Ignored ${w}`);
        } else if (w.startsWith("-")) {
          if (!["-s", "--silent", "-S", "--show-error", "-v", "--verbose", "-i", "--include", "--compressed", "-sS", "-f", "--fail"].includes(w)) {
            warnings.push(`Ignored ${w}`);
          }
        } else if (!url) {
          url = w;
        }
    }
  }

  if (!url) throw new Error("No URL found in the cURL command.");

  const r = newRequest();
  r.url = url;
  r.name = (() => {
    try {
      const u = new URL(url.includes("://") ? url : `http://${url}`);
      const last = u.pathname.split("/").filter(Boolean).pop();
      return last ? `${(method ?? (data.length || form.length ? "POST" : "GET"))} ${last}` : u.host;
    } catch {
      return "Imported request";
    }
  })();
  r.headers = headers.filter((h) => !(form.length && h.key.toLowerCase() === "content-type"));

  let body: Body = { mode: "none" };
  if (get && data.length) {
    r.url += (r.url.includes("?") ? "&" : "?") + data.join("&");
  } else if (form.length) {
    body = { mode: "multipart", fields: form.map((f) => ({ ...f, enabled: true })) };
  } else if (data.length) {
    const text = data.join("&");
    const ct = headers.find((h) => h.key.toLowerCase() === "content-type")?.value.toLowerCase() ?? "";
    const looksJson = /^\s*[[{]/.test(text);
    if (ct.includes("json") || (!ct && looksJson)) {
      let pretty = text;
      try {
        pretty = JSON.stringify(JSON.parse(text), null, 2);
      } catch {
        // keep as typed
      }
      body = { mode: "json", text: pretty };
      r.headers = r.headers.filter((h) => !(h.key.toLowerCase() === "content-type" && h.value.toLowerCase().includes("json")));
    } else if (ct.includes("x-www-form-urlencoded") || (!ct && (urlencode || /^[^=&\s]+=[^&]*(&[^=&\s]+=[^&]*)*$/.test(text)))) {
      body = {
        mode: "form-urlencoded",
        fields: text.split("&").map((pair) => {
          const eq = pair.indexOf("=");
          const dec = (v: string) => {
            try {
              return decodeURIComponent(v.replace(/\+/g, " "));
            } catch {
              return v;
            }
          };
          return { key: dec(eq >= 0 ? pair.slice(0, eq) : pair), value: dec(eq >= 0 ? pair.slice(eq + 1) : ""), enabled: true };
        }),
      };
      r.headers = r.headers.filter((h) => h.key.toLowerCase() !== "content-type");
    } else if (ct.includes("xml")) {
      body = { mode: "xml", text };
    } else {
      body = { mode: "text", text, contentType: ct || undefined };
      r.headers = r.headers.filter((h) => h.key.toLowerCase() !== "content-type");
    }
  }
  r.body = body;
  r.method = method ?? (get ? "GET" : body.mode !== "none" ? "POST" : "GET");

  // Authorization: Bearer xyz → bearer auth.
  const authHeader = r.headers.find((h) => h.key.toLowerCase() === "authorization");
  if (authHeader && /^bearer\s+/i.test(authHeader.value) && auth.type === "inherit") {
    auth = { type: "bearer", token: authHeader.value.replace(/^bearer\s+/i, "") };
    r.headers = r.headers.filter((h) => h !== authHeader);
  }
  r.auth = auth;
  r.settings = { followRedirects: true, verifyTls: !insecure };
  return { request: r, warnings };
}
