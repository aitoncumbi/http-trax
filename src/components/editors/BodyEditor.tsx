"use client";

import { Wand2 } from "lucide-react";

import type { Body, BodyMode, MultipartField } from "@/lib/types";
import { prettyJson } from "@/lib/util";

import { Radio } from "../ui/Controls";
import { CodeEditor } from "./CodeEditor";
import { KeyValueEditor, makeKv } from "./KeyValueEditor";
import { useVarScope } from "./VarScope";

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
  const variables = useVarScope();
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-[38px] shrink-0 items-center justify-between gap-3 border-b border-border-soft px-4">
        <div role="radiogroup" aria-label="Body type" className="flex min-w-0 items-center gap-[18px] overflow-x-auto">
          {MODES.map((m) => (
            <Radio key={m.id} checked={body.mode === m.id} onChange={() => onChange(switchMode(body, m.id))}>
              {m.label}
            </Radio>
          ))}
        </div>
        {body.mode === "json" && (
          <button
            type="button"
            className="btn-ghost shrink-0"
            title="Format JSON"
            onClick={() => {
              const pretty = prettyJson(body.text);
              if (pretty) onChange({ ...body, text: pretty });
            }}
          >
            <Wand2 size={14} strokeWidth={1.5} /> Beautify
          </button>
        )}
        {body.mode === "text" && (
          <input
            className="input h-7 w-56 shrink-0 font-mono text-[12px]"
            placeholder="Content-Type (text/plain)"
            aria-label="Content type"
            value={body.contentType ?? ""}
            onChange={(e) => onChange({ ...body, contentType: e.target.value || undefined })}
          />
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {body.mode === "none" && <div className="flex h-full items-center justify-center text-[14px] text-faint">This request has no body.</div>}
        {(body.mode === "json" || body.mode === "xml" || body.mode === "text") && (
          <CodeEditor
            value={body.text}
            language={body.mode === "json" ? "json" : body.mode === "xml" ? "xml" : "text"}
            variables={variables}
            onChange={(text) => onChange({ ...body, text })}
            placeholder={body.mode === "json" ? '{\n  "name": "{{userName}}"\n}' : ""}
          />
        )}
        {body.mode === "form-urlencoded" && (
          <div className="px-4 py-3">
            <KeyValueEditor rows={body.fields} onChange={(fields) => onChange({ ...body, fields })} make={makeKv} />
          </div>
        )}
        {body.mode === "multipart" && (
          <div className="px-4 py-3">
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
              <div className="eyebrow px-4 pt-3">Query</div>
              <CodeEditor value={body.query} variables={variables} onChange={(query) => onChange({ ...body, query })} placeholder="query { users { id name } }" />
            </div>
            <div className="min-h-0">
              <div className="eyebrow px-4 pt-3">Variables (JSON)</div>
              <CodeEditor value={body.variables} language="json" variables={variables} onChange={(v) => onChange({ ...body, variables: v })} placeholder='{ "id": "{{userId}}" }' />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
