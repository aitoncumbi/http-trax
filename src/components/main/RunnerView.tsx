"use client";

import { Check, ChevronDown, ChevronRight, FileSpreadsheet, Loader2, RotateCw, X } from "lucide-react";
import { useRef, useState } from "react";

import { useStore, type RunnerTab } from "@/lib/store";
import type { RunnerOptions } from "@/lib/types";
import { cx, formatDuration, statusColor } from "@/lib/util";

import { MethodPill } from "../ui/Brand";
import { PageHeader } from "./PageHeader";

/** Iterations, delay, data file and stop-on-failure for the next run. */
function RunOptionsBar({ tab }: { tab: RunnerTab }) {
  const { updateTab } = useStore.getState();
  const file = useRef<HTMLInputElement>(null);
  const o = tab.options;
  const set = (patch: Partial<RunnerOptions>) => updateTab<RunnerTab>(tab.id, (t) => ({ ...t, options: { ...t.options, ...patch } }));
  const number = (v: string) => (v === "" ? undefined : Math.max(0, Math.floor(Number(v))) || undefined);

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-x-5 gap-y-2 border-b border-border px-5 py-2.5 text-[13px]">
      <label className="flex items-center gap-2 text-muted">
        Iterations
        <input
          type="number"
          min={1}
          placeholder={o.data ? "per row" : "1"}
          value={o.iterations ?? ""}
          onChange={(e) => set({ iterations: number(e.target.value) })}
          className="input h-7 w-20 text-right font-mono text-[12.5px]"
        />
      </label>
      <label className="flex items-center gap-2 text-muted">
        Delay
        <input
          type="number"
          min={0}
          placeholder="0"
          value={o.delayMs ?? ""}
          onChange={(e) => set({ delayMs: number(e.target.value) })}
          className="input h-7 w-20 text-right font-mono text-[12.5px]"
        />
        ms
      </label>
      <div className="flex items-center gap-2">
        <input
          ref={file}
          type="file"
          accept=".csv,.json,text/csv,application/json"
          className="hidden"
          aria-label="Data file"
          onChange={async (e) => {
            const f = e.target.files?.[0];
            e.target.value = "";
            if (f) set({ data: { name: f.name, text: await f.text() } });
          }}
        />
        <button type="button" className="btn-ghost h-7 px-2" onClick={() => file.current?.click()} title="Run once per row of a CSV or JSON file">
          <FileSpreadsheet size={14} strokeWidth={1.6} /> {o.data ? o.data.name : "Data file…"}
        </button>
        {o.data && (
          <button type="button" className="btn-ghost h-7 w-7 justify-center p-0" aria-label="Remove data file" onClick={() => set({ data: null })}>
            <X size={13} />
          </button>
        )}
      </div>
      <label className="flex items-center gap-2 text-muted">
        <input type="checkbox" checked={!!o.stopOnFailure} onChange={(e) => set({ stopOnFailure: e.target.checked })} />
        Stop on first failure
      </label>
    </div>
  );
}

export function RunnerView({ tab }: { tab: RunnerTab }) {
  const { rerun, openRequest } = useStore.getState();
  const [open, setOpen] = useState<number | null>(null);
  const report = tab.run?.report;
  const tests = report ? report.testsPassed + report.testsFailed : 0;
  const iterations = report?.iterations ?? 1;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader eyebrow="COLLECTION RUN" title={tab.groupName}>
        <button type="button" className="btn h-9 px-3.5" onClick={() => void rerun(tab.id)} disabled={tab.running}>
          {tab.running ? <Loader2 size={14} className="animate-spin" /> : <RotateCw size={14} strokeWidth={1.6} />} Run again
        </button>
      </PageHeader>
      <RunOptionsBar tab={tab} />

      {tab.error && <p className="mx-5 mb-3 rounded-lg bg-danger-soft p-3 text-[13px] text-danger">{tab.error}</p>}
      {tab.running && !report && (
        <div className="flex flex-1 items-center justify-center gap-2 text-[13.5px] text-muted">
          <Loader2 size={16} className="animate-spin" /> Running requests…
        </div>
      )}

      {report && (
        <>
          <div className="flex shrink-0 items-baseline gap-6 border-b border-border px-5 pb-4">
            <div className="flex items-baseline gap-2.5">
              <span className="text-[21px] font-semibold tracking-[-0.02em]">
                {report.succeeded} of {report.total} passed
              </span>
              {report.failed > 0 && <span className="text-[13px] text-danger">{report.failed} failed</span>}
            </div>
            {tests > 0 && (
              <span className={cx("text-[13px]", report.testsFailed ? "text-danger" : "text-ok")}>
                {report.testsPassed}/{tests} tests
              </span>
            )}
            <span className="text-[13px] text-faint">{report.environment ? report.environment : "No environment"}</span>
            {iterations > 1 && <span className="text-[13px] text-faint">{iterations} iterations</span>}
            <span className="text-[13px] text-faint">{formatDuration(report.durationMs)}</span>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-3 py-1.5">
            {report.items.map((item, i) => {
              const result = tab.run?.results[i];
              const expanded = open === i;
              const iteration = item.iteration ?? 0;
              const newIteration = iterations > 1 && (i === 0 || (report.items[i - 1].iteration ?? 0) !== iteration);
              return (
                <div key={i} className="border-b border-border-soft">
                  {newIteration && (
                    <div className="px-2 pt-3 pb-1 text-[11.5px] font-semibold tracking-[0.06em] text-faint uppercase">
                      Iteration {iteration + 1} of {iterations}
                    </div>
                  )}
                  <button
                    type="button"
                    aria-expanded={expanded}
                    className="flex h-11 w-full items-center gap-3 rounded-lg px-2 text-left hover:bg-hover"
                    onClick={() => setOpen(expanded ? null : i)}
                  >
                    {expanded ? <ChevronDown size={14} className="text-faint" /> : <ChevronRight size={14} className="text-faint" />}
                    <span
                      className={cx(
                        "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full",
                        item.ok ? "bg-ok/15 text-ok" : "bg-danger/15 text-danger",
                      )}
                    >
                      {item.ok ? <Check size={12} strokeWidth={2.4} /> : <X size={12} strokeWidth={2.4} />}
                    </span>
                    <MethodPill method={item.method} />
                    <span className="min-w-0 flex-1 truncate text-[14px]">{item.name}</span>
                    {item.failed + item.passed > 0 && (
                      <span className={cx("text-[12.5px]", item.failed ? "text-danger" : "text-muted")}>
                        {item.passed}/{item.passed + item.failed} tests
                      </span>
                    )}
                    <span className={cx("w-12 text-right font-mono text-[12.5px] font-semibold", item.status ? statusColor(item.status) : "text-faint")}>
                      {item.status ?? "—"}
                    </span>
                    <span className="w-16 text-right text-[12.5px] text-muted">
                      {item.timeMs !== null && item.timeMs !== undefined ? formatDuration(item.timeMs) : ""}
                    </span>
                  </button>
                  {expanded && result && (
                    <div className="space-y-2 pr-3 pb-3 pl-[70px] text-[13px]">
                      <div className="font-mono text-[12.5px] break-all text-muted">{item.url}</div>
                      {item.error && <div className="rounded-lg bg-danger-soft px-3 py-2 font-mono text-[12.5px] text-danger">{item.error}</div>}
                      {result.tests.map((t, j) => (
                        <div key={j} className="flex items-start gap-2">
                          {t.passed ? <Check size={14} className="mt-0.5 text-ok" /> : <X size={14} className="mt-0.5 text-danger" />}
                          <span>
                            {t.name}
                            {t.error && <span className="block font-mono text-[12px] text-danger">{t.error}</span>}
                          </span>
                        </div>
                      ))}
                      <button type="button" className="btn-ghost -ml-2 h-7" onClick={() => void openRequest(item.requestId)}>
                        Open request
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
