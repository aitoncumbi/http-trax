"use client";

import { Wand2 } from "lucide-react";

import type { Body, BodyMode, MultipartField } from "@/lib/types";
import { prettyJson } from "@/lib/util";

import { CodeEditor } from "./CodeEditor";
import { KeyValueEditor, makeKv } from "./KeyValueEditor";

const MODES: { id: BodyMode; label: string }[] = [
  { id: "none", label: "None" },
  { id: "json", label: "JSON" },
  { id: "text", label: "Text" },
  { id: "xml", label: "XML" },
  { id: "form-urlencoded", label: "Form URL-encoded" },
  { id: "multipart", label: "Multipart form" },
  { id: "graphql", label: "GraphQL" },
];

function textOf(body: Body): string {
  switch (body.mode) {
    case "json":
    case "text":
    case "xml":
      return body.text;
    case "graphql":
      return body.query;
    default:
      return "";
  }
}

function switchMode(body: Body, mode: BodyMode): Body {
  const text = textOf(body);
  switch (mode) {
    case "none":
      return { mode };
    case "json":
    case "xml":
      return { mode, text };
    case "text":
      return { mode, text };
    case "graphql":
      return { mode, query: text, variables: "" };
    case "form-urlencoded":
      return { mode, fields: body.mode === "multipart" ? body.fields.filter((f) => !f.isFile).map(({ key, value, enabled }) => ({ key, value, enabled })) : [] };
    case "multipart":
      return { mode, fields: body.mode === "form-urlencoded" ? body.fields.map(({ key, value, enabled }) => ({ key, value, enabled })) : [] };
  }
}

export function BodyEditor({ body, onChange }: { body: Body; onChange: (b: Body) => void }) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-1 px-3 py-2">
        {MODES.map((m) => (
          <label key={m.id} className="flex cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-muted hover:bg-hover has-[:checked]:text-text">
            <input type="radio" name="body-mode" checked={body.mode === m.id} onChange={() => onChange(switchMode(body, m.id))} className="accent-[var(--accent)]" />
            {m.label}
          </label>
        ))}
        {body.mode === "json" && (
          <button
            className="btn-ghost ml-auto"
            title="Format JSON"
            onClick={() => {
              const pretty = prettyJson(body.text);
              if (pretty) onChange({ ...body, text: pretty });
            }}
          >
            <Wand2 size={12} /> Beautify
          </button>
        )}
        {body.mode === "text" && (
          <input
            className="input ml-auto h-7 w-56 font-mono text-[11.5px]"
            placeholder="Content-Type (text/plain)"
            value={body.contentType ?? ""}
            onChange={(e) => onChange({ ...body, contentType: e.target.value || undefined })}
          />
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto border-t border-border">
        {body.mode === "none" && <p className="p-4 text-[12.5px] text-faint">This request has no body.</p>}
        {(body.mode === "json" || body.mode === "xml" || body.mode === "text") && (
          <CodeEditor
            value={body.text}
            language={body.mode === "json" ? "json" : body.mode === "xml" ? "xml" : "text"}
            onChange={(text) => onChange({ ...body, text })}
            placeholder={body.mode === "json" ? '{\n  "name": "{{userName}}"\n}' : ""}
          />
        )}
        {body.mode === "form-urlencoded" && (
          <div className="p-3">
            <KeyValueEditor rows={body.fields} onChange={(fields) => onChange({ ...body, fields })} make={makeKv} />
          </div>
        )}
        {body.mode === "multipart" && (
          <div className="p-3">
            <KeyValueEditor<MultipartField>
              rows={body.fields}
              files
              onChange={(fields) => onChange({ ...body, fields })}
              make={() => ({ key: "", value: "", enabled: true, isFile: false })}
            />
          </div>
        )}
        {body.mode === "graphql" && (
          <div className="grid h-full min-h-0 grid-cols-[2fr_1fr]">
            <div className="min-h-0 border-r border-border">
              <div className="label px-3 pt-2">Query</div>
              <CodeEditor value={body.query} onChange={(query) => onChange({ ...body, query })} placeholder="query { users { id name } }" />
            </div>
            <div className="min-h-0">
              <div className="label px-3 pt-2">Variables (JSON)</div>
              <CodeEditor value={body.variables} language="json" onChange={(variables) => onChange({ ...body, variables })} placeholder='{ "id": "{{userId}}" }' />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
