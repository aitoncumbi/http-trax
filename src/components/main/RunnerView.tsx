"use client";

import { CheckCircle2, ChevronDown, ChevronRight, Loader2, RotateCw, XCircle } from "lucide-react";
import { useState } from "react";

import { useStore, type RunnerTab } from "@/lib/store";
import { cx, formatDuration, methodColor, statusColor } from "@/lib/util";

export function RunnerView({ tab }: { tab: RunnerTab }) {
  const { rerun, openRequest } = useStore.getState();
  const [open, setOpen] = useState<string | null>(null);
  const report = tab.run?.report;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-semibold tracking-wider text-faint uppercase">Collection run</div>
          <div className="text-[16px] font-semibold">{tab.groupName}</div>
        </div>
        <button className="btn" onClick={() => void rerun(tab.id)} disabled={tab.running}>
          {tab.running ? <Loader2 size={14} className="animate-spin" /> : <RotateCw size={14} />} Run again
        </button>
      </div>

      {tab.error && <p className="m-4 rounded-md bg-danger-soft p-3 text-[12.5px] text-danger">{tab.error}</p>}
      {tab.running && !report && (
        <div className="flex flex-1 items-center justify-center gap-2 text-muted">
          <Loader2 size={16} className="animate-spin" /> Running requests…
        </div>
      )}

      {report && (
        <>
          <div className="grid grid-cols-4 gap-3 px-4 py-4">
            {[
              { label: "Requests", value: report.total, tone: "" },
              { label: "Passed", value: report.succeeded, tone: "text-ok" },
              { label: "Failed", value: report.failed, tone: report.failed ? "text-danger" : "" },
              { label: "Tests", value: `${report.testsPassed}/${report.testsPassed + report.testsFailed}`, tone: report.testsFailed ? "text-danger" : "text-ok" },
            ].map((m) => (
              <div key={m.label} className="rounded-lg border border-border bg-panel p-3">
                <div className="text-[11px] text-faint">{m.label}</div>
                <div className={cx("text-[20px] font-semibold tabular-nums", m.tone)}>{m.value}</div>
              </div>
            ))}
          </div>
          <div className="px-4 pb-2 text-[12px] text-faint">
            {report.environment ? `Environment: ${report.environment}` : "No environment"} · {formatDuration(report.durationMs)}
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-2 pb-4">
            {report.items.map((item, i) => {
              const result = tab.run?.results[i];
              const expanded = open === item.requestId;
              return (
                <div key={item.requestId} className="border-b border-border">
                  <button className="flex w-full items-center gap-3 px-2 py-2 text-left hover:bg-hover" onClick={() => setOpen(expanded ? null : item.requestId)}>
                    {expanded ? <ChevronDown size={14} className="text-faint" /> : <ChevronRight size={14} className="text-faint" />}
                    {item.ok ? <CheckCircle2 size={15} className="text-ok" /> : <XCircle size={15} className="text-danger" />}
                    <span className={cx("w-14 font-mono text-[11px] font-bold", methodColor(item.method))}>{item.method}</span>
                    <span className="min-w-0 flex-1 truncate">{item.name}</span>
                    {item.failed + item.passed > 0 && (
                      <span className={cx("text-[12px]", item.failed ? "text-danger" : "text-muted")}>
                        {item.passed}/{item.passed + item.failed} tests
                      </span>
                    )}
                    <span className={cx("w-12 text-right font-mono text-[12px] font-bold", item.status ? statusColor(item.status) : "text-faint")}>{item.status ?? "—"}</span>
                    <span className="w-16 text-right text-[12px] text-muted">{item.timeMs !== null && item.timeMs !== undefined ? formatDuration(item.timeMs) : ""}</span>
                  </button>
                  {expanded && result && (
                    <div className="space-y-2 px-10 pb-3 text-[12px]">
                      <div className="font-mono break-all text-muted">{item.url}</div>
                      {item.error && <div className="rounded bg-danger-soft px-2 py-1.5 font-mono text-danger">{item.error}</div>}
                      {result.tests.map((t, j) => (
                        <div key={j} className="flex items-start gap-2">
                          {t.passed ? <CheckCircle2 size={13} className="mt-0.5 text-ok" /> : <XCircle size={13} className="mt-0.5 text-danger" />}
                          <span>
                            {t.name}
                            {t.error && <span className="block font-mono text-[11.5px] text-danger">{t.error}</span>}
                          </span>
                        </div>
                      ))}
                      <button className="btn-ghost" onClick={() => void openRequest(item.requestId)}>
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
