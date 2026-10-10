// Code snippets from an exported request (see `export_request`): cURL, fetch,
// axios, Python requests, Go net/http and Rust reqwest.

import type { ExportRequest } from "./types";

export type SnippetLang = "curl" | "fetch" | "axios" | "python" | "go" | "rust";

export const SNIPPET_LANGS: { id: SnippetLang; label: string; editor: "text" | "javascript" }[] = [
  { id: "curl", label: "cURL", editor: "text" },
  { id: "fetch", label: "fetch", editor: "javascript" },
  { id: "axios", label: "axios", editor: "javascript" },
  { id: "python", label: "Python", editor: "text" },
  { id: "go", label: "Go", editor: "text" },
  { id: "rust", label: "Rust", editor: "text" },
];

/** POSIX shell single-quoting. */
export function shellQuote(s: string): string {
  return /^[\w@%+=:,./-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`;
}

/** Double-quoted string literal valid in JS, Python and Go. */
const q = (s: string) => JSON.stringify(s);

/** Rust string literal; raw when it reads better. */
function rustStr(s: string): string {
  if (/["\\\n]/.test(s) && !s.includes('"#')) return `r#"${s}"#`;
  return `"${s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n").replace(/\r/g, "\\r").replace(/\t/g, "\\t")}"`;
}

function contentType(req: ExportRequest): string {
  return req.headers.find((h) => h.key.toLowerCase() === "content-type")?.value.toLowerCase() ?? "";
}

/** The body as a JSON value, when it is JSON (so snippets can use native objects). */
function jsonBody(req: ExportRequest): unknown | undefined {
  if (req.body.kind !== "text" || !contentType(req).includes("json")) return undefined;
  try {
    return JSON.parse(req.body.text);
  } catch {
    return undefined;
  }
}

function indent(text: string, by: string): string {
  return text
    .split("\n")
    .map((l, i) => (i === 0 ? l : by + l))
    .join("\n");
}

// ------------------------------------------------------------------ cURL

function curl(req: ExportRequest): string {
  const parts: string[] = [];
  const hasBody = req.body.kind !== "none";
  if (req.method === "HEAD") parts.push("--head");
  else if (req.method !== "GET" || hasBody) parts.push(`-X ${req.method}`);
  parts.push(shellQuote(req.url));
  for (const h of req.headers) parts.push(`-H ${shellQuote(`${h.key}: ${h.value}`)}`);
  if (req.basicAuth) parts.push(`-u ${shellQuote(`${req.basicAuth.username}:${req.basicAuth.password}`)}`);
  switch (req.body.kind) {
    case "text":
      parts.push(`--data-raw ${shellQuote(req.body.text)}`);
      break;
    case "form":
      for (const [k, v] of req.body.fields) parts.push(`--data-urlencode ${shellQuote(`${k}=${v}`)}`);
      break;
    case "multipart":
      for (const p of req.body.parts) parts.push(`-F ${shellQuote(`${p.name}=${p.isFile ? "@" : ""}${p.value}`)}`);
      break;
  }
  return `curl ${parts.join(" \\\n  ")}`;
}

// ------------------------------------------------------------- JS (shared)

function jsHeaders(req: ExportRequest, basicInHeader: boolean): string[] {
  const lines = req.headers.map((h) => `${q(h.key)}: ${q(h.value)},`);
  if (basicInHeader && req.basicAuth) {
    lines.push(`"Authorization": "Basic " + btoa(${q(`${req.basicAuth.username}:${req.basicAuth.password}`)}),`);
  }
  return lines;
}

function jsForm(req: ExportRequest): { prelude: string[]; expr: string } | null {
  if (req.body.kind !== "multipart") return null;
  const files = req.body.parts.some((p) => p.isFile);
  const prelude = [...(files ? ['import { openAsBlob } from "node:fs";', ""] : []), "const form = new FormData();"];
  for (const p of req.body.parts) {
    prelude.push(p.isFile ? `form.append(${q(p.name)}, await openAsBlob(${q(p.value)}), ${q(p.value.split(/[\\/]/).pop() ?? p.value)});` : `form.append(${q(p.name)}, ${q(p.value)});`);
  }
  return { prelude: [...prelude, ""], expr: "form" };
}

function fetchSnippet(req: ExportRequest): string {
  const form = jsForm(req);
  const lines = [...(form?.prelude ?? [])];
  const opts: string[] = [`method: ${q(req.method)},`];
  const headers = jsHeaders(req, true);
  if (headers.length) opts.push(`headers: {\n${headers.map((h) => `    ${h}`).join("\n")}\n  },`);
  const json = jsonBody(req);
  if (json !== undefined) opts.push(`body: JSON.stringify(${indent(JSON.stringify(json, null, 2), "  ")}),`);
  else if (req.body.kind === "text") opts.push(`body: ${q(req.body.text)},`);
  else if (req.body.kind === "form") {
    opts.push(`body: new URLSearchParams({\n${req.body.fields.map(([k, v]) => `    ${q(k)}: ${q(v)},`).join("\n")}\n  }),`);
  } else if (form) opts.push(`body: ${form.expr},`);
  lines.push(`const response = await fetch(${q(req.url)}, {\n${opts.map((o) => `  ${o}`).join("\n")}\n});`);
  lines.push("");
  lines.push("console.log(response.status, await response.text());");
  return lines.join("\n");
}

function axiosSnippet(req: ExportRequest): string {
  const form = jsForm(req);
  const lines = ['import axios from "axios";', "", ...(form?.prelude.filter((l) => !l.startsWith("import")) ?? [])];
  if (form?.prelude.some((l) => l.startsWith("import"))) lines.splice(1, 0, 'import { openAsBlob } from "node:fs";');
  const opts: string[] = [`method: ${q(req.method.toLowerCase())},`, `url: ${q(req.url)},`];
  const headers = jsHeaders(req, false);
  if (headers.length) opts.push(`headers: {\n${headers.map((h) => `    ${h}`).join("\n")}\n  },`);
  if (req.basicAuth) opts.push(`auth: { username: ${q(req.basicAuth.username)}, password: ${q(req.basicAuth.password)} },`);
  const json = jsonBody(req);
  if (json !== undefined) opts.push(`data: ${indent(JSON.stringify(json, null, 2), "  ")},`);
  else if (req.body.kind === "text") opts.push(`data: ${q(req.body.text)},`);
  else if (req.body.kind === "form") {
    opts.push(`data: new URLSearchParams({\n${req.body.fields.map(([k, v]) => `    ${q(k)}: ${q(v)},`).join("\n")}\n  }),`);
  } else if (form) opts.push(`data: ${form.expr},`);
  lines.push(`const response = await axios({\n${opts.map((o) => `  ${o}`).join("\n")}\n});`);
  lines.push("");
  lines.push("console.log(response.status, response.data);");
  return lines.join("\n");
}

// ---------------------------------------------------------------- Python

/** A JSON value as a Python literal. */
export function pyLiteral(v: unknown, pad = ""): string {
  if (v === null) return "None";
  if (v === true) return "True";
  if (v === false) return "False";
  if (typeof v === "number") return String(v);
  if (typeof v === "string") return q(v);
  const inner = pad + "    ";
  if (Array.isArray(v)) {
    if (v.length === 0) return "[]";
    return `[\n${v.map((x) => `${inner}${pyLiteral(x, inner)},`).join("\n")}\n${pad}]`;
  }
  const entries = Object.entries(v as Record<string, unknown>);
  if (entries.length === 0) return "{}";
  return `{\n${entries.map(([k, x]) => `${inner}${q(k)}: ${pyLiteral(x, inner)},`).join("\n")}\n${pad}}`;
}

function python(req: ExportRequest): string {
  const lines = ["import requests", "", `url = ${q(req.url)}`];
  const args = ["url"];
  if (req.headers.length) {
    lines.push(`headers = {\n${req.headers.map((h) => `    ${q(h.key)}: ${q(h.value)},`).join("\n")}\n}`);
    args.push("headers=headers");
  }
  const json = jsonBody(req);
  if (json !== undefined) {
    lines.push(`payload = ${pyLiteral(json)}`);
    args.push("json=payload");
  } else if (req.body.kind === "text") {
    lines.push(`payload = ${q(req.body.text)}`);
    args.push("data=payload");
  } else if (req.body.kind === "form") {
    lines.push(`payload = {\n${req.body.fields.map(([k, v]) => `    ${q(k)}: ${q(v)},`).join("\n")}\n}`);
    args.push("data=payload");
  } else if (req.body.kind === "multipart") {
    const files = req.body.parts.filter((p) => p.isFile);
    const fields = req.body.parts.filter((p) => !p.isFile);
    if (fields.length) {
      lines.push(`payload = {\n${fields.map((p) => `    ${q(p.name)}: ${q(p.value)},`).join("\n")}\n}`);
      args.push("data=payload");
    }
    if (files.length) {
      lines.push(`files = {\n${files.map((p) => `    ${q(p.name)}: open(${q(p.value)}, "rb"),`).join("\n")}\n}`);
      args.push("files=files");
    }
  }
  if (req.basicAuth) args.push(`auth=(${q(req.basicAuth.username)}, ${q(req.basicAuth.password)})`);
  lines.push("");
  lines.push(`response = requests.request(${q(req.method)}, ${args.join(", ")})`);
  lines.push("print(response.status_code)");
  lines.push("print(response.text)");
  return lines.join("\n");
}

// -------------------------------------------------------------------- Go

function goStr(s: string): string {
  return s.includes("\n") && !s.includes("`") ? `\`${s}\`` : q(s);
}

function go(req: ExportRequest): string {
  const imports = new Set(["fmt", "io", "net/http"]);
  const body: string[] = [];
  let reader = "nil";
  let contentTypeExpr: string | null = null;
  switch (req.body.kind) {
    case "text":
      imports.add("strings");
      body.push(`\tbody := strings.NewReader(${goStr(req.body.text)})`);
      reader = "body";
      break;
    case "form":
      imports.add("strings");
      imports.add("net/url");
      body.push("\tform := url.Values{}");
      for (const [k, v] of req.body.fields) body.push(`\tform.Set(${q(k)}, ${q(v)})`);
      body.push("\tbody := strings.NewReader(form.Encode())");
      reader = "body";
      break;
    case "multipart":
      imports.add("bytes");
      imports.add("mime/multipart");
      body.push("\tvar buf bytes.Buffer", "\tw := multipart.NewWriter(&buf)");
      for (const p of req.body.parts) {
        if (p.isFile) {
          imports.add("os");
          imports.add("path/filepath");
          body.push(
            `\tif f, err := os.Open(${q(p.value)}); err == nil {`,
            `\t\tpart, _ := w.CreateFormFile(${q(p.name)}, filepath.Base(${q(p.value)}))`,
            "\t\tio.Copy(part, f)",
            "\t\tf.Close()",
            "\t}",
          );
        } else body.push(`\tw.WriteField(${q(p.name)}, ${q(p.value)})`);
      }
      body.push("\tw.Close()");
      reader = "&buf";
      contentTypeExpr = "w.FormDataContentType()";
      break;
  }
  const lines = ["package main", "", "import ("];
  for (const i of [...imports].sort()) lines.push(`\t${q(i)}`);
  lines.push(")", "", "func main() {", ...body);
  if (body.length) lines.push("");
  lines.push(`\treq, err := http.NewRequest(${q(req.method)}, ${q(req.url)}, ${reader})`, "\tif err != nil {", "\t\tpanic(err)", "\t}");
  for (const h of req.headers) lines.push(`\treq.Header.Set(${q(h.key)}, ${q(h.value)})`);
  if (contentTypeExpr) lines.push(`\treq.Header.Set("Content-Type", ${contentTypeExpr})`);
  if (req.basicAuth) lines.push(`\treq.SetBasicAuth(${q(req.basicAuth.username)}, ${q(req.basicAuth.password)})`);
  lines.push(
    "",
    "\tres, err := http.DefaultClient.Do(req)",
    "\tif err != nil {",
    "\t\tpanic(err)",
    "\t}",
    "\tdefer res.Body.Close()",
    "\tdata, _ := io.ReadAll(res.Body)",
    "\tfmt.Println(res.Status)",
    "\tfmt.Println(string(data))",
    "}",
  );
  return lines.join("\n");
}

// ------------------------------------------------------------------ Rust

function rust(req: ExportRequest): string {
  const features = ["blocking"];
  const pre: string[] = [];
  const shortcut: Record<string, string> = { GET: "get", POST: "post", PUT: "put", PATCH: "patch", DELETE: "delete", HEAD: "head" };
  const call = shortcut[req.method]
    ? `.${shortcut[req.method]}(${rustStr(req.url)})`
    : `.request(reqwest::Method::from_bytes(${rustStr(req.method)}.as_bytes())?, ${rustStr(req.url)})`;
  const chain = [call];
  for (const h of req.headers) chain.push(`.header(${rustStr(h.key)}, ${rustStr(h.value)})`);
  if (req.basicAuth) chain.push(`.basic_auth(${rustStr(req.basicAuth.username)}, Some(${rustStr(req.basicAuth.password)}))`);
  switch (req.body.kind) {
    case "text":
      chain.push(`.body(${rustStr(req.body.text)})`);
      break;
    case "form":
      chain.push(`.form(&[\n${req.body.fields.map(([k, v]) => `            (${rustStr(k)}, ${rustStr(v)}),`).join("\n")}\n        ])`);
      break;
    case "multipart": {
      features.push("multipart");
      const parts = req.body.parts.map((p) => (p.isFile ? `\n        .file(${rustStr(p.name)}, ${rustStr(p.value)})?` : `\n        .text(${rustStr(p.name)}, ${rustStr(p.value)})`));
      pre.push(`    let form = reqwest::blocking::multipart::Form::new()${parts.join("")};`);
      chain.push(".multipart(form)");
      break;
    }
  }
  chain.push(".send()?;");
  return [
    `// Cargo.toml: reqwest = { version = "0.12", features = [${features.map((f) => `"${f}"`).join(", ")}] }`,
    "fn main() -> Result<(), Box<dyn std::error::Error>> {",
    ...pre,
    "    let client = reqwest::blocking::Client::new();",
    `    let response = client\n${chain.map((c) => `        ${c}`).join("\n")}`,
    '    println!("{}", response.status());',
    '    println!("{}", response.text()?);',
    "    Ok(())",
    "}",
  ].join("\n");
}

export function generate(lang: SnippetLang, req: ExportRequest): string {
  switch (lang) {
    case "curl":
      return curl(req);
    case "fetch":
      return fetchSnippet(req);
    case "axios":
      return axiosSnippet(req);
    case "python":
      return python(req);
    case "go":
      return go(req);
    case "rust":
      return rust(req);
  }
}
