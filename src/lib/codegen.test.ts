import { describe, expect, it } from "vitest";

import { generate, pyLiteral, shellQuote } from "./codegen";
import { parseCurl } from "./curl";
import type { ExportRequest } from "./types";

const json: ExportRequest = {
  method: "POST",
  url: "https://api.example.com/v1/orders?dry=1",
  headers: [
    { key: "Content-Type", value: "application/json" },
    { key: "Authorization", value: "Bearer s3cr3t" },
  ],
  body: { kind: "text", text: '{"sku": "TRX-001", "qty": 2, "gift": false, "note": null}' },
  basicAuth: null,
  warnings: [],
};

const form: ExportRequest = {
  method: "POST",
  url: "https://example.com/login",
  headers: [{ key: "Content-Type", value: "application/x-www-form-urlencoded" }],
  body: { kind: "form", fields: [["user", "ada"], ["pass", "it's"]] },
  basicAuth: { username: "ada", password: "pw" },
  warnings: [],
};

const multipart: ExportRequest = {
  method: "PUT",
  url: "https://example.com/upload",
  headers: [],
  body: {
    kind: "multipart",
    parts: [
      { name: "note", value: "hi", isFile: false },
      { name: "file", value: "/tmp/a b.png", isFile: true },
    ],
  },
  warnings: [],
};

describe("shellQuote", () => {
  it("leaves safe words and quotes the rest", () => {
    expect(shellQuote("https://a.b/c?d=1")).toBe("'https://a.b/c?d=1'");
    expect(shellQuote("abc-1.2")).toBe("abc-1.2");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
  });
});

describe("cURL", () => {
  it("round-trips through the cURL importer", () => {
    const text = generate("curl", json);
    expect(text).toContain("-X POST");
    const { request } = parseCurl(text);
    expect(request.method).toBe("POST");
    expect(request.url).toBe("https://api.example.com/v1/orders?dry=1");
    expect(request.body?.mode).toBe("json");
    expect(JSON.parse(request.body?.mode === "json" ? request.body.text : "")).toEqual({ sku: "TRX-001", qty: 2, gift: false, note: null });
    expect(request.auth).toEqual({ type: "bearer", token: "s3cr3t" });
  });

  it("uses -u, --data-urlencode and -F", () => {
    const f = generate("curl", form);
    expect(f).toContain("-u ada:pw");
    expect(f).toContain("--data-urlencode 'pass=it'\\''s'");
    const m = generate("curl", multipart);
    expect(m).toContain("-F note=hi");
    expect(m).toContain("-F 'file=@/tmp/a b.png'");
  });

  it("omits -X for a plain GET", () => {
    expect(generate("curl", { ...json, method: "GET", body: { kind: "none" }, headers: [] })).toBe("curl 'https://api.example.com/v1/orders?dry=1'");
  });
});

describe("JavaScript", () => {
  it("fetch sends JSON as an object and basic auth as a header", () => {
    const f = generate("fetch", json);
    expect(f).toContain('method: "POST"');
    expect(f).toContain('"sku": "TRX-001"');
    expect(f).toContain("body: JSON.stringify(");
    expect(generate("fetch", form)).toContain('"Authorization": "Basic " + btoa("ada:pw")');
    expect(generate("fetch", form)).toContain("new URLSearchParams");
    const m = generate("fetch", multipart);
    expect(m).toContain('import { openAsBlob } from "node:fs";');
    expect(m).toContain('form.append("file", await openAsBlob("/tmp/a b.png"), "a b.png");');
  });

  it("axios uses auth and data", () => {
    const a = generate("axios", form);
    expect(a.startsWith('import axios from "axios";')).toBe(true);
    expect(a).toContain('auth: { username: "ada", password: "pw" }');
    expect(generate("axios", json)).toContain('method: "post"');
  });
});

describe("Python", () => {
  it("maps JSON to Python literals", () => {
    expect(pyLiteral({ a: true, b: [null, 1] })).toBe('{\n    "a": True,\n    "b": [\n        None,\n        1,\n    ],\n}');
    const p = generate("python", json);
    expect(p).toContain("json=payload");
    expect(p).toContain('"gift": False');
    expect(p).toContain('requests.request("POST", url, headers=headers, json=payload)');
  });

  it("uses data, files and auth", () => {
    expect(generate("python", form)).toContain('auth=("ada", "pw")');
    const m = generate("python", multipart);
    expect(m).toContain('"file": open("/tmp/a b.png", "rb")');
    expect(m).toContain("data=payload, files=files");
  });
});

describe("Go and Rust", () => {
  it("Go imports only what it uses", () => {
    const g = generate("go", form);
    expect(g).toContain('"net/url"');
    expect(g).not.toContain('"bytes"');
    expect(g).toContain('req.SetBasicAuth("ada", "pw")');
    const m = generate("go", multipart);
    expect(m).toContain('"mime/multipart"');
    expect(m).toContain("w.FormDataContentType()");
  });

  it("Rust uses reqwest's helpers", () => {
    const r = generate("rust", json);
    expect(r).toContain('.post("https://api.example.com/v1/orders?dry=1")');
    expect(r).toContain('.body(r#"{"sku": "TRX-001"');
    expect(generate("rust", form)).toContain('.basic_auth("ada", Some("pw"))');
    const m = generate("rust", multipart);
    expect(m).toContain('features = ["blocking", "multipart"]');
    expect(m).toContain('.file("file", "/tmp/a b.png")?');
    expect(m).toContain('.put("https://example.com/upload")');
  });
});
