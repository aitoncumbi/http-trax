"use client";

import { Bookmark, Check, Copy, Loader2, Minus, Trash2, X } from "lucide-react";
import { useState } from "react";

import { useStore, type RequestTab } from "@/lib/store";
import type { ResponseData, SentRequest } from "@/lib/types";
import { cx, formatDuration, formatSize, methodColor, prettyJson, statusColor, timeAgo } from "@/lib/util";

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

function statusDot(status: number): string {
  if (status >= 500) return "bg-danger";
  if (status >= 400) return "bg-warn";
  if (status >= 300) return "bg-info";
  return "bg-ok";
}

function HeaderRows({ headers, dense }: { headers: { key: string; value: string }[]; dense?: boolean }) {
  return (
    <div className="font-mono text-[13px]">
      {headers.map((h, i) => (
        <div key={i} className={cx("flex items-center gap-2.5 px-2.5", dense ? "h-7" : "h-[38px] border-t border-border-soft")}>
          <span className="w-[34%] shrink-0 truncate text-key" title={h.key}>
            {h.key}
          </span>
          <span className="min-w-0 flex-1 truncate text-muted" title={h.value}>
            {h.value}
          </span>
        </div>
      ))}
    </div>
  );
}

function RequestSummary({ req }: { req: SentRequest }) {
  return (
    <div className="px-5 py-4">
      <div className="font-mono text-[13px] break-all">
        <span className={cx("font-semibold", methodColor(req.method))}>{req.method}</span> {req.url}
      </div>
      <div className="mt-3.5 -mx-2.5">
        <HeaderRows headers={req.headers} dense />
      </div>
      {req.body && (
        <>
          <div className="eyebrow mt-5 mb-2">Body</div>
          <pre className="max-h-[320px] overflow-auto rounded-lg border border-border bg-panel p-3 font-mono text-[12.5px] whitespace-pre-wrap text-muted">
            {req.body}
          </pre>
        </>
      )}
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      className="btn-ghost h-[26px]"
      title="Copy body"
      onClick={() => {
        void navigator.clipboard?.writeText(text);
        setDone(true);
        setTimeout(() => setDone(false), 1200);
      }}
    >
      {done ? <Check size={14} strokeWidth={1.6} /> : <Copy size={14} strokeWidth={1.4} />}
      {done ? "Copied" : "Copy"}
    </button>
  );
}

/** Prepended to HTML previews: no scripts (sandbox), no outside requests, links can't navigate. */
const PREVIEW_HEAD =
  '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; style-src \'unsafe-inline\'; img-src data:; font-src data:"><base target="_blank">';

function HtmlPreview({ html, url }: { html: string; url: string }) {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 px-5 pt-2 pb-4">
      <div className="flex min-h-[160px] flex-1 flex-col overflow-hidden rounded-[10px] border border-border-mid">
        <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border-mid bg-sunken px-3">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-faint" aria-hidden>
            <rect x="3.5" y="7" width="9" height="6.5" rx="1.5" />
            <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" />
          </svg>
          <span className="min-w-0 truncate font-mono text-[12px] text-faint">{url}</span>
        </div>
        <iframe title="HTML preview" sandbox="" srcDoc={PREVIEW_HEAD + html} className="min-h-0 w-full flex-1 bg-white" />
      </div>
      <div className="text-[12px] text-faint">Rendered in a sandbox. Scripts and outside requests are blocked.</div>
    </div>
  );
}

function BodyView({ response }: { response: ResponseData }) {
  const ct = contentType(response);
  const isHtml = ct.includes("text/html") || (!ct && /^\s*<(!doctype html|html)/i.test(response.body));
  const [mode, setMode] = useState<"pretty" | "raw" | "preview">(isHtml ? "preview" : "pretty");
  if (response.bodyBase64) {
    if (ct.startsWith("image/")) {
      return (
        <div className="flex flex-1 items-center justify-center overflow-auto p-4">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={`data:${ct};base64,${response.bodyBase64}`} alt="Response body" className="max-h-full max-w-full rounded border border-border" />
        </div>
      );
    }
    return <Empty title="Binary body" text={`${formatSize(response.size)} of ${ct || "unknown type"}.`} />;
  }
  if (!response.body) return <Empty title="Empty body" text="The server sent no content." />;
  const lang = languageFor(ct, response.body);
  const pretty = lang === "json" ? (prettyJson(response.body) ?? response.body) : response.body;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2.5 px-5 pt-2.5 pb-1">
        <Pills<"pretty" | "raw" | "preview">
          options={[
            { id: "pretty", label: "Pretty" },
            { id: "raw", label: "Raw" },
            ...(isHtml ? [{ id: "preview" as const, label: "Preview" }] : []),
          ]}
          value={mode}
          onChange={setMode}
        />
        <span className="min-w-0 flex-1 truncate text-[12.5px] text-faint">{ct}</span>
        <CopyButton text={response.body} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        {mode === "preview" ? (
          <HtmlPreview html={response.body} url={response.url} />
        ) : mode === "pretty" ? (
          <CodeEditor value={pretty} language={lang} readOnly wrap />
        ) : (
          <pre className="px-5 py-3 font-mono text-[13px] leading-[21px] break-all whitespace-pre-wrap text-muted">{response.body}</pre>
        )}
      </div>
    </div>
  );
}

function Empty({ title, text, children }: { title: string; text?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex min-h-[160px] flex-1 flex-col items-center justify-center gap-1.5 px-6 text-center">
      <div className="text-[15px] font-medium">{title}</div>
      {text && <div className="max-w-[340px] text-[13px] leading-[19px] text-faint">{text}</div>}
      {children}
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
  const canSave = !saved && !!result?.response;

  const header = (
    <div className="flex h-[34px] shrink-0 items-center justify-between gap-3 pr-2.5 pl-4">
      <div className="flex min-w-0 items-center gap-4 overflow-hidden whitespace-nowrap">
        <span className="eyebrow">{saved ? "SAVED RESPONSE" : "RESPONSE"}</span>
        {tab.running && (
          <span className="flex items-center gap-1.5 text-[13px] text-muted">
            <Loader2 size={13} className="animate-spin" /> Sending…
          </span>
        )}
        {!tab.running && response && (
          <>
            <span className={cx("inline-flex items-center gap-1.5 font-mono text-[12.5px] font-semibold", statusColor(response.status))}>
              <span className={cx("h-1.5 w-1.5 rounded-full", statusDot(response.status))} aria-hidden />
              {response.status} {response.statusText}
            </span>
            <span className="text-[13px] text-muted">{formatDuration(response.timeMs)}</span>
            <span className="text-[13px] text-muted">{formatSize(response.size)}</span>
            {tests.length > 0 && (
              <span className={cx("inline-flex items-center gap-[5px] text-[13px]", failed ? "text-danger" : "text-ok")}>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <circle cx="8" cy="8" r="5.5" />
                  {failed ? <path d="m6 6 4 4m0-4-4 4" /> : <path d="m5.6 8.2 1.7 1.7 3.1-3.4" />}
                </svg>
                {passed}/{tests.length} tests
              </span>
            )}
          </>
        )}
        {!tab.running && !response && result?.error && <span className="truncate text-[13px] text-danger">{result.error}</span>}
        {!tab.running && result?.skipped && !saved && <span className="text-[13px] text-warn">Skipped by the pre-request script</span>}
        {saved && <span className="truncate text-[13px] text-faint">“{saved.name}” · {timeAgo(saved.savedAt)}</span>}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {saved && (
          <button type="button" className="btn-ghost h-7" onClick={() => updateTab<RequestTab>(tab.id, (t) => ({ ...t, viewingSaved: null }))}>
            Back to last run
          </button>
        )}
        {canSave && (
          <button
            type="button"
            className="btn-ghost h-7"
            onClick={() => void saveResponse(tab.id)}
            title="Keep this response as an example next to the request (stored in git)"
          >
            <Bookmark size={14} strokeWidth={1.4} /> Save response
          </button>
        )}
      </div>
    </div>
  );

  const tabs = (
    <TabStrip<Section>
      label="Response sections"
      value={section}
      onChange={setSection}
      className="px-1"
      tabs={[
        { id: "body", label: "Body" },
        { id: "headers", label: "Headers", badge: response?.headers.length },
        ...(!saved
          ? [
              { id: "tests" as const, label: "Tests", badge: tests.length ? `${passed}/${tests.length}` : undefined, tone: failed ? ("danger" as const) : ("ok" as const) },
              { id: "console" as const, label: "Console", badge: logs.length + scriptErrors.length || undefined, tone: scriptErrors.length ? ("danger" as const) : undefined },
            ]
          : []),
        { id: "request", label: "Request" },
        { id: "saved", label: "Saved", badge: tab.saved.length || undefined },
      ]}
    />
  );

  const savedList = (
    <SavedList
      tab={tab}
      onPick={() => setSection("body")}
      onDelete={(id) => void deleteSavedResponse(tab.id, id)}
      onSaveCurrent={canSave ? () => void saveResponse(tab.id) : undefined}
    />
  );

  if (!result && !saved && !tab.running) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        {header}
        {tabs}
        {section === "saved" ? (
          savedList
        ) : (
          <Empty title="No response yet" text="Send the request to see what comes back.">
            <p className="mt-2 text-[12.5px] text-faint">
              <span className="kbd">Ctrl</span> <span className="kbd">Enter</span>
            </p>
          </Empty>
        )}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {header}
      {tabs}
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        {section === "body" &&
          (response ? (
            <BodyView key={saved?.id ?? result?.id} response={response} />
          ) : tab.running ? (
            <Empty title="Waiting for the server…" />
          ) : (
            <div className="space-y-2 px-5 py-4">
              {result?.error && <p className="rounded-lg bg-danger-soft p-3 font-mono text-[12.5px] text-danger">{result.error}</p>}
              {scriptErrors.map((e, i) => (
                <p key={i} className="rounded-lg bg-danger-soft p-3 font-mono text-[12.5px] text-danger">
                  {e.phase} script ({e.source}): {e.message}
                </p>
              ))}
              {result?.skipped && <p className="text-[13px] text-faint">The pre-request script called skipRequest(), so nothing was sent.</p>}
            </div>
          ))}
        {section === "headers" &&
          (response ? (
            <div className="px-3 pt-2 pb-3">
              <HeaderRows headers={response.headers} />
            </div>
          ) : (
            <Empty title="No headers" text="No response was received." />
          ))}
        {section === "tests" &&
          (tests.length === 0 ? (
            <Empty title="No tests" text="Write them under Scripts → Post-response with trax.test()." />
          ) : (
            <div className="px-5 py-4">
              <div className="mb-2.5 flex items-baseline gap-2.5">
                <span className="text-[21px] font-semibold tracking-[-0.02em]">
                  {passed} of {tests.length} passed
                </span>
                {failed > 0 && <span className="text-[12.5px] text-danger">{failed} failed</span>}
              </div>
              {tests.map((t, i) => (
                <div key={i} className="flex min-h-[46px] items-center gap-3 border-t border-border-soft py-2">
                  <span
                    className={cx(
                      "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full",
                      t.skipped ? "bg-hover text-faint" : t.passed ? "bg-ok/15 text-ok" : "bg-danger/15 text-danger",
                    )}
                  >
                    {t.skipped ? <Minus size={12} strokeWidth={2.2} /> : t.passed ? <Check size={12} strokeWidth={2.4} /> : <X size={12} strokeWidth={2.4} />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="text-[14px]">{t.name}</div>
                    {t.error && <div className="mt-0.5 font-mono text-[12px] break-words text-danger">{t.error}</div>}
                  </div>
                  {t.source !== "request" && <span className="shrink-0 font-mono text-[12px] text-faint">{t.source.replace(":", ": ")}</span>}
                </div>
              ))}
            </div>
          ))}
        {section === "console" &&
          (logs.length === 0 && scriptErrors.length === 0 ? (
            <Empty title="Nothing logged" text="console.log() output from pre-request and post-response scripts shows up here." />
          ) : (
            <div className="px-5 py-2 font-mono text-[13px]">
              {scriptErrors.map((e, i) => (
                <div key={`e${i}`} className="flex min-h-[38px] items-center gap-3 border-b border-border-soft py-1.5">
                  <span className="rounded bg-danger/15 px-1.5 py-px text-[11px] text-danger">error</span>
                  <span className="text-faint">{e.phase}</span>
                  <span className="min-w-0 flex-1 break-all text-danger">{e.message}</span>
                </div>
              ))}
              {logs.map((l, i) => (
                <div key={i} className="flex min-h-[38px] items-center gap-3 border-b border-border-soft py-1.5">
                  <span
                    className={cx(
                      "shrink-0 rounded px-1.5 py-px text-[11px]",
                      l.level === "error" ? "bg-danger/15 text-danger" : l.level === "warn" ? "bg-warn/15 text-warn" : "bg-accent-soft text-accent-text",
                    )}
                  >
                    {l.level === "log" || l.level === "info" ? l.phase : l.level}
                  </span>
                  <span
                    className={cx(
                      "min-w-0 flex-1 break-all whitespace-pre-wrap",
                      l.level === "error" ? "text-danger" : l.level === "warn" ? "text-warn" : "text-text",
                    )}
                  >
                    {l.message}
                  </span>
                  {l.source !== "request" && <span className="shrink-0 text-[12px] text-faint">{l.source}</span>}
                </div>
              ))}
            </div>
          ))}
        {section === "request" && (sent ? <RequestSummary req={sent} /> : <Empty title="Nothing sent yet" />)}
        {section === "saved" && savedList}
      </div>
    </div>
  );
}

function SavedList({
  tab,
  onPick,
  onDelete,
  onSaveCurrent,
}: {
  tab: RequestTab;
  onPick: () => void;
  onDelete: (id: string) => void;
  onSaveCurrent?: () => void;
}) {
  const updateTab = useStore((s) => s.updateTab);
  if (tab.saved.length === 0) {
    return (
      <Empty title="No saved responses" text="Save a response to keep it as an example next to this request.">
        {onSaveCurrent && (
          <button type="button" className="btn mt-2.5 h-[34px] px-4" onClick={onSaveCurrent}>
            Save current response
          </button>
        )}
      </Empty>
    );
  }
  return (
    <div className="px-3 py-2">
      {tab.saved.map((s) => (
        <div
          key={s.id}
          className={cx("group flex h-10 items-center gap-3 rounded-lg px-2.5 hover:bg-hover", tab.viewingSaved === s.id && "bg-selected")}
        >
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-3 text-left"
            onClick={() => {
              updateTab<RequestTab>(tab.id, (t) => ({ ...t, viewingSaved: s.id }));
              onPick();
            }}
          >
            <span className={cx("w-10 font-mono text-[12.5px] font-semibold", statusColor(s.response.status))}>{s.response.status}</span>
            <span className="min-w-0 flex-1 truncate text-[14px]">{s.name}</span>
            <span className="text-[12px] text-faint">{timeAgo(s.savedAt)}</span>
          </button>
          <button type="button" className="icon-btn h-7 w-7 opacity-0 group-hover:opacity-100 focus:opacity-100" title="Delete saved response" onClick={() => onDelete(s.id)}>
            <Trash2 size={14} strokeWidth={1.4} />
          </button>
        </div>
      ))}
      {onSaveCurrent && (
        <button type="button" className="btn-ghost mt-2 h-8" onClick={onSaveCurrent}>
          <Bookmark size={14} strokeWidth={1.4} /> Save current response
        </button>
      )}
    </div>
  );
}
