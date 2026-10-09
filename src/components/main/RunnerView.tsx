"use client";

import { Check, ChevronDown, ChevronRight, Loader2, RotateCw, X } from "lucide-react";
import { useState } from "react";

import { useStore, type RunnerTab } from "@/lib/store";
import { cx, formatDuration, statusColor } from "@/lib/util";

import { MethodPill } from "../ui/Brand";
import { PageHeader } from "./PageHeader";

export function RunnerView({ tab }: { tab: RunnerTab }) {
  const { rerun, openRequest } = useStore.getState();
  const [open, setOpen] = useState<string | null>(null);
  const report = tab.run?.report;
  const tests = report ? report.testsPassed + report.testsFailed : 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader eyebrow="COLLECTION RUN" title={tab.groupName}>
        <button type="button" className="btn h-9 px-3.5" onClick={() => void rerun(tab.id)} disabled={tab.running}>
          {tab.running ? <Loader2 size={14} className="animate-spin" /> : <RotateCw size={14} strokeWidth={1.6} />} Run again
        </button>
      </PageHeader>

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
            <span className="text-[13px] text-faint">{formatDuration(report.durationMs)}</span>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-3 py-1.5">
            {report.items.map((item, i) => {
              const result = tab.run?.results[i];
              const expanded = open === item.requestId;
              return (
                <div key={item.requestId} className="border-b border-border-soft">
                  <button
                    type="button"
                    aria-expanded={expanded}
                    className="flex h-11 w-full items-center gap-3 rounded-lg px-2 text-left hover:bg-hover"
                    onClick={() => setOpen(expanded ? null : item.requestId)}
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
