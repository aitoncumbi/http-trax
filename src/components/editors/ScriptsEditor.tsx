"use client";

import type { ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { useRef, useState } from "react";

import type { Scripts } from "@/lib/types";
import { cx } from "@/lib/util";

import { CodeEditor } from "./CodeEditor";

type Phase = "preRequest" | "postResponse";

const SNIPPETS: Record<Phase, { label: string; code: string }[]> = {
  preRequest: [
    { label: "Set a header", code: 'trax.request.headers.set("X-Request-Id", trax.uuid());\n' },
    { label: "Use a timestamp", code: 'trax.vars.set("now", new Date().toISOString());\n' },
    { label: "Basic auth header", code: 'trax.request.headers.set("Authorization", "Basic " + btoa(trax.env.get("user") + ":" + trax.env.get("password")));\n' },
    { label: "Skip if no token", code: 'if (!trax.env.get("token")) {\n  console.warn("no token; skipping");\n  trax.skipRequest();\n}\n' },
    { label: "Build a JSON body", code: 'trax.request.body = JSON.stringify({\n  id: trax.uuid(),\n  createdAt: new Date().toISOString(),\n});\n' },
  ],
  postResponse: [
    { label: "Status is 200", code: 'trax.test("status is 200", () => {\n  trax.expect(trax.response.status).toBe(200);\n});\n' },
    { label: "Response time < 500 ms", code: 'trax.test("fast enough", () => {\n  trax.expect(trax.response.time).toBeLessThan(500);\n});\n' },
    { label: "JSON field equals", code: 'trax.test("has the right name", () => {\n  const body = trax.response.json();\n  trax.expect(body).toHaveProperty("name", "Ada");\n});\n' },
    { label: "Header present", code: 'trax.test("is JSON", () => {\n  trax.expect(trax.response.headers.get("content-type")).toContain("application/json");\n});\n' },
    { label: "Save value to environment", code: 'trax.env.set("token", trax.response.json().access_token);\n' },
    { label: "Save value for this session", code: 'trax.vars.set("userId", trax.response.json().id);\n' },
    { label: "Log the body", code: "console.log(trax.response.json());\n" },
  ],
};

const HELP: Record<Phase, string> = {
  preRequest: "Runs before the request is sent. Change trax.request (url, method, headers, body) or set variables. {{variables}} are resolved after this script.",
  postResponse: "Runs after the response arrives. Write tests with trax.test() and trax.expect(), or store values for the next request.",
};

export function ScriptsEditor({
  scripts,
  onChange,
  scopeNote,
}: {
  scripts: Scripts;
  onChange: (s: Scripts) => void;
  scopeNote?: string;
}) {
  const [phase, setPhase] = useState<Phase>(scripts.postResponse && !scripts.preRequest ? "postResponse" : "preRequest");
  const ref = useRef<ReactCodeMirrorRef>(null);
  const value = scripts[phase] ?? "";

  const insert = (code: string) => {
    const view = ref.current?.view;
    if (view) {
      const { from, to } = view.state.selection.main;
      const prefix = from > 0 && view.state.doc.sliceString(from - 1, from) !== "\n" ? "\n" : "";
      view.dispatch({ changes: { from, to, insert: prefix + code }, selection: { anchor: from + prefix.length + code.length } });
      view.focus();
    } else {
      onChange({ ...scripts, [phase]: value + (value && !value.endsWith("\n") ? "\n" : "") + code });
    }
  };

  const phases: { id: Phase; label: string; filled: boolean }[] = [
    { id: "preRequest", label: "Pre-request", filled: !!scripts.preRequest?.trim() },
    { id: "postResponse", label: "Post-response (tests)", filled: !!scripts.postResponse?.trim() },
  ];
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div role="tablist" aria-label="Script phase" className="flex h-[42px] shrink-0 items-stretch gap-5 border-b border-border-soft px-5">
        {phases.map((p) => (
          <button
            key={p.id}
            type="button"
            role="tab"
            aria-selected={phase === p.id}
            onClick={() => setPhase(p.id)}
            className={cx(
              "-mb-px inline-flex items-center gap-2 border-b-2 text-[13.5px]",
              phase === p.id ? "border-accent-line font-medium text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {p.label}
            <span className={cx("h-1.5 w-1.5 rounded-full", p.filled ? "bg-faint" : "bg-transparent")} aria-hidden />
          </button>
        ))}
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 min-w-0 flex-1 overflow-auto">
          <CodeEditor
            key={phase}
            ref={ref}
            value={value}
            language="javascript"
            onChange={(v) => onChange({ ...scripts, [phase]: v })}
            placeholder={phase === "preRequest" ? "// e.g. trax.request.headers.set('X-Trace', trax.uuid())" : "// e.g. trax.test('ok', () => trax.expect(trax.response.status).toBe(200))"}
          />
        </div>
        <aside aria-label="Script help" className="w-[224px] shrink-0 overflow-y-auto border-l border-border bg-rail px-[18px] py-4 text-[12.5px] leading-[18px] text-faint">
          <p>{HELP[phase]}</p>
          <p className="mt-2.5">{scopeNote ?? "Collection and folder scripts run first."}</p>
          <div className="mt-[18px] mb-2 text-[11px] font-semibold tracking-[0.06em]">SNIPPETS</div>
          {SNIPPETS[phase].map((s) => (
            <button
              key={s.label}
              type="button"
              className="block h-[30px] w-full rounded-md text-left text-[13px] text-accent-text hover:text-accent-text-2"
              onClick={() => insert(s.code)}
            >
              {s.label}
            </button>
          ))}
          <p className="mt-3 text-[11.5px] leading-[17px]">
            Postman-style <code className="font-mono">pm.*</code>, <code className="font-mono">expect(x).to.equal(y)</code>,{" "}
            <code className="font-mono">await</code> and <code className="font-mono">btoa</code> also work.
          </p>
        </aside>
      </div>
    </div>
  );
}
