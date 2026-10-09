"use client";

import { Bookmark, CheckCircle2, CircleSlash, Copy, History, Loader2, Trash2, XCircle } from "lucide-react";
import { useState } from "react";

import { useStore, type RequestTab } from "@/lib/store";
import type { ResponseData, SentRequest } from "@/lib/types";
import { cx, formatDuration, formatSize, prettyJson, statusColor, timeAgo } from "@/lib/util";

import { CodeEditor, type CodeLanguage } from "../editors/CodeEditor";
import { Pills, TabStrip } from "../ui/Segmented";

type Section = "body" | "headers" | "tests" | "console" | "request" | "saved";

function contentType(r: ResponseData): string {
  return r.headers.find((h) => h.key.toLowerCase() === "content-type")?.value ?? "";
}

function languageFor(ct: string, body: string): CodeLanguage {
  if (ct.includes("json") || /^\s*[[{]/.test(body)) return "json";
  if (ct.includes("xml") || ct.includes("html")) return "xml";
  return "text";
}

function HeaderTable({ headers }: { headers: { key: string; value: string }[] }) {
  return (
    <table className="w-full border-collapse font-mono text-[12px]">
      <tbody>
        {headers.map((h, i) => (
          <tr key={i} className="border-b border-border align-top">
            <td className="w-1/3 py-1.5 pr-3 pl-4 text-muted">{h.key}</td>
            <td className="py-1.5 pr-4 break-all">{h.value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function RequestSummary({ req }: { req: SentRequest }) {
  return (
    <div className="space-y-3 p-4">
      <div className="font-mono text-[12.5px] break-all">
        <span className="font-bold">{req.method}</span> {req.url}
      </div>
      <div>
        <div className="label">Headers</div>
        <div className="rounded-md border border-border">
          <HeaderTable headers={req.headers} />
        </div>
      </div>
      {req.body && (
        <div>
          <div className="label">Body</div>
          <pre className="max-h-[300px] overflow-auto rounded-md border border-border bg-panel-2 p-3 font-mono text-[12px] whitespace-pre-wrap">{req.body}</pre>
        </div>
      )}
    </div>
  );
}

function BodyView({ response }: { response: ResponseData }) {
  const [mode, setMode] = useState<"pretty" | "raw">("pretty");
  const ct = contentType(response);
  if (response.bodyBase64) {
    if (ct.startsWith("image/")) {
      return (
        <div className="flex flex-1 items-center justify-center overflow-auto p-4">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={`data:${ct};base64,${response.bodyBase64}`} alt="Response body" className="max-h-full max-w-full rounded border border-border" />
        </div>
      );
    }
    return <p className="p-4 text-[12.5px] text-faint">Binary body ({formatSize(response.size)}, {ct || "unknown type"}).</p>;
  }
  if (!response.body) return <p className="p-4 text-[12.5px] text-faint">Empty body.</p>;
  const lang = languageFor(ct, response.body);
  const pretty = lang === "json" ? (prettyJson(response.body) ?? response.body) : response.body;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Pills
          options={[
            { id: "pretty", label: "Pretty" },
            { id: "raw", label: "Raw" },
          ]}
          value={mode}
          onChange={setMode}
        />
        <span className="text-[11px] text-faint">{ct}</span>
        <button className="btn-ghost ml-auto" onClick={() => void navigator.clipboard?.writeText(response.body)} title="Copy body">
          <Copy size={12} /> Copy
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto border-t border-border">
        {mode === "pretty" ? (
          <CodeEditor value={pretty} language={lang} readOnly wrap />
        ) : (
          <pre className="p-3 font-mono text-[12px] break-all whitespace-pre-wrap">{response.body}</pre>
        )}
      </div>
    </div>
  );
}

export function ResponseViewer({ tab }: { tab: RequestTab }) {
  const [section, setSection] = useState<Section>("body");
  const { saveResponse, deleteSavedResponse, updateTab } = useStore.getState();
  const result = tab.result;
  const saved = tab.viewingSaved ? tab.saved.find((s) => s.id === tab.viewingSaved) : undefined;
  const response = saved ? saved.response : result?.response;
  const sent = saved ? saved.request : result?.request;

  const tests = saved ? [] : (result?.tests ?? []);
  const passed = tests.filter((t) => t.passed && !t.skipped).length;
  const failed = tests.filter((t) => !t.passed).length;
  const logs = saved ? [] : (result?.logs ?? []);
  const scriptErrors = saved ? [] : (result?.scriptErrors ?? []);

  const header = (
    <div className="flex h-10 shrink-0 items-center gap-3 border-b border-border px-4 text-[12.5px]">
      <span className="text-[11px] font-semibold tracking-wider text-faint uppercase">{saved ? "Saved response" : "Response"}</span>
      {tab.running && (
        <span className="flex items-center gap-1.5 text-muted">
          <Loader2 size={13} className="animate-spin" /> Sending…
        </span>
      )}
      {!tab.running && response && (
        <>
          <span className={cx("font-mono font-bold", statusColor(response.status))}>
            {response.status} {response.statusText}
          </span>
          <span className="text-muted">{formatDuration(response.timeMs)}</span>
          <span className="text-muted">{formatSize(response.size)}</span>
          {tests.length > 0 && (
            <span className={cx("flex items-center gap-1", failed ? "text-danger" : "text-ok")}>
              {failed ? <XCircle size={13} /> : <CheckCircle2 size={13} />}
              {passed}/{tests.length} tests
            </span>
          )}
        </>
      )}
      {!tab.running && !response && result?.error && <span className="truncate text-danger">{result.error}</span>}
      {!tab.running && result?.skipped && !saved && <span className="text-warn">Skipped by pre-request script</span>}
      <div className="ml-auto flex items-center gap-1">
        {saved && (
          <button className="btn-ghost" onClick={() => updateTab<RequestTab>(tab.id, (t) => ({ ...t, viewingSaved: null }))}>
            Back to last run
          </button>
        )}
        {!saved && result?.response && (
          <button className="btn-ghost" onClick={() => void saveResponse(tab.id)} title="Keep this response as an example on the request (stored in git)">
            <Bookmark size={13} /> Save response
          </button>
        )}
      </div>
    </div>
  );

  if (!result && !saved && !tab.running) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        {header}
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-[12.5px] text-faint">
          <p>
            Press <span className="kbd">Ctrl</span>+<span className="kbd">Enter</span> to send.
          </p>
          {tab.saved.length > 0 && (
            <button className="btn-ghost" onClick={() => setSection("saved")}>
              <History size={13} /> {tab.saved.length} saved response{tab.saved.length > 1 ? "s" : ""}
            </button>
          )}
        </div>
        {section === "saved" && <SavedList tab={tab} onPick={() => setSection("body")} onDelete={(id) => void deleteSavedResponse(tab.id, id)} />}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {header}
      {saved && (
        <div className="border-b border-border bg-info/10 px-4 py-1.5 text-[12px] text-info">
          “{saved.name}” · saved {timeAgo(saved.savedAt)}
        </div>
      )}
      <TabStrip<Section>
        value={section}
        onChange={setSection}
        tabs={[
          { id: "body", label: "Body" },
          { id: "headers", label: "Headers", badge: response?.headers.length },
          ...(!saved ? [{ id: "tests" as const, label: "Tests", badge: tests.length ? `${passed}/${tests.length}` : undefined }] : []),
          ...(!saved ? [{ id: "console" as const, label: "Console", badge: logs.length + scriptErrors.length || undefined }] : []),
          { id: "request", label: "Request" },
          { id: "saved", label: "Saved", badge: tab.saved.length || undefined },
        ]}
      />
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        {section === "body" &&
          (response ? (
            <BodyView response={response} />
          ) : (
            <div className="p-4 text-[12.5px]">
              {result?.error && <p className="rounded-md bg-danger-soft p-3 font-mono text-danger">{result.error}</p>}
              {scriptErrors.map((e, i) => (
                <p key={i} className="mt-2 rounded-md bg-danger-soft p-3 font-mono text-[12px] text-danger">
                  {e.phase} script ({e.source}): {e.message}
                </p>
              ))}
            </div>
          ))}
        {section === "headers" && response && <HeaderTable headers={response.headers} />}
        {section === "tests" && (
          <div className="p-2">
            {tests.length === 0 && <p className="p-2 text-[12.5px] text-faint">No tests. Add some in Scripts → Post-response.</p>}
            {tests.map((t, i) => (
              <div key={i} className="flex items-start gap-2 rounded-md px-2 py-1.5 hover:bg-hover">
                {t.skipped ? (
                  <CircleSlash size={14} className="mt-0.5 text-faint" />
                ) : t.passed ? (
                  <CheckCircle2 size={14} className="mt-0.5 text-ok" />
                ) : (
                  <XCircle size={14} className="mt-0.5 text-danger" />
                )}
                <div className="min-w-0 flex-1">
                  <div className="text-[12.5px]">{t.name}</div>
                  {t.error && <div className="font-mono text-[11.5px] text-danger">{t.error}</div>}
                </div>
                {t.source !== "request" && <span className="text-[11px] text-faint">{t.source.replace(":", ": ")}</span>}
              </div>
            ))}
          </div>
        )}
        {section === "console" && (
          <div className="p-2 font-mono text-[12px]">
            {logs.length === 0 && scriptErrors.length === 0 && <p className="p-2 font-sans text-[12.5px] text-faint">Nothing logged.</p>}
            {scriptErrors.map((e, i) => (
              <div key={`e${i}`} className="border-b border-border px-2 py-1 text-danger">
                ✗ {e.phase} ({e.source}): {e.message}
              </div>
            ))}
            {logs.map((l, i) => (
              <div
                key={i}
                className={cx(
                  "flex gap-2 border-b border-border px-2 py-1",
                  l.level === "error" && "text-danger",
                  l.level === "warn" && "text-warn",
                )}
              >
                <span className="w-24 shrink-0 text-faint">{l.phase}</span>
                <span className="break-all whitespace-pre-wrap">{l.message}</span>
              </div>
            ))}
          </div>
        )}
        {section === "request" && sent && <RequestSummary req={sent} />}
        {section === "saved" && <SavedList tab={tab} onPick={() => setSection("body")} onDelete={(id) => void deleteSavedResponse(tab.id, id)} />}
      </div>
    </div>
  );
}

function SavedList({ tab, onPick, onDelete }: { tab: RequestTab; onPick: () => void; onDelete: (id: string) => void }) {
  const updateTab = useStore((s) => s.updateTab);
  if (tab.saved.length === 0) {
    return <p className="p-4 text-[12.5px] text-faint">No saved responses yet. Send the request, then “Save response” to keep it as an example.</p>;
  }
  return (
    <div className="p-2">
      {tab.saved.map((s) => (
        <div
          key={s.id}
          className={cx("group flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover", tab.viewingSaved === s.id && "bg-accent-soft")}
        >
          <button
            className="flex min-w-0 flex-1 items-center gap-3 text-left"
            onClick={() => {
              updateTab<RequestTab>(tab.id, (t) => ({ ...t, viewingSaved: s.id }));
              onPick();
            }}
          >
            <span className={cx("w-10 font-mono text-[12px] font-bold", statusColor(s.response.status))}>{s.response.status}</span>
            <span className="flex-1 truncate">{s.name}</span>
            <span className="text-[11px] text-faint">{timeAgo(s.savedAt)}</span>
          </button>
          <button className="icon-btn opacity-0 group-hover:opacity-100" title="Delete saved response" onClick={() => onDelete(s.id)}>
            <Trash2 size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
