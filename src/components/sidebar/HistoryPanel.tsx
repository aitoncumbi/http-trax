"use client";

import { Eraser, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";

import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { HistoryEntry } from "@/lib/types";
import { cx, statusColor } from "@/lib/util";

import { MethodPill } from "../ui/Brand";

function dayLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((start(today) - start(d)) / 86_400_000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: diff < 7 ? "long" : undefined, month: "short", day: "numeric" });
}

/** Every run in this workspace, including runs made by AI agents over MCP. */
export function HistoryPanel() {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const epoch = useStore((s) => s.historyEpoch);
  const exists = useStore((s) => s.tree);
  const { openRequest, confirm } = useStore.getState();
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .getHistory(workspaceId)
      .then((h) => !cancelled && setEntries(h))
      .catch(() => !cancelled && setEntries([]));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, epoch, reload]);

  // Agents run requests from another process; refresh when the window is focused.
  useEffect(() => {
    const onFocus = () => setReload((n) => n + 1);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const known = new Set<string>();
  const walk = (items: NonNullable<typeof exists>["collections"][number]["items"]) =>
    items.forEach((i) => (i.kind === "request" ? known.add(i.id) : walk(i.items)));
  exists?.collections.forEach((c) => walk(c.items));

  const groups: { label: string; items: HistoryEntry[] }[] = [];
  for (const e of entries ?? []) {
    const label = dayLabel(e.at);
    const last = groups[groups.length - 1];
    if (last?.label === label) last.items.push(e);
    else groups.push({ label, items: [e] });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center justify-between pt-3.5 pr-1 pb-2 pl-2.5">
        <h2 className="text-[15px] font-semibold">History</h2>
        <div className="flex">
          <button type="button" className="icon-btn" aria-label="Refresh history" title="Refresh" onClick={() => setReload((n) => n + 1)}>
            <RefreshCw size={14} strokeWidth={1.6} />
          </button>
          {!!entries?.length && workspaceId && (
            <button
              type="button"
              className="icon-btn"
              aria-label="Clear history"
              title="Clear history"
              onClick={async () => {
                if (!(await confirm({ title: "Clear history?", message: "Removes the local run history for this workspace.", confirm: "Clear" }))) return;
                await api.clearHistory(workspaceId);
                setReload((n) => n + 1);
              }}
            >
              <Eraser size={14} strokeWidth={1.6} />
            </button>
          )}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-3">
        {entries && entries.length === 0 && (
          <p className="px-2.5 py-2 text-[13px] leading-[19px] text-faint">
            Requests you send, and requests AI agents run through MCP, show up here. History stays on this machine.
          </p>
        )}
        {groups.map((g) => (
          <div key={g.label}>
            <div className="px-2.5 pt-2.5 pb-1.5 text-[12px] font-semibold tracking-[0.06em] text-faint uppercase">{g.label}</div>
            <div className="flex flex-col gap-0.5 pb-2">
              {g.items.map((e) => {
                const openable = !!e.requestId && known.has(e.requestId);
                const time = new Date(e.at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
                return (
                  <button
                    key={e.id}
                    type="button"
                    disabled={!openable}
                    onClick={() => {
                      if (e.requestId) void openRequest(e.requestId);
                    }}
                    title={`${e.method} ${e.url}\n${time}${e.environment ? ` · ${e.environment}` : ""}${e.error ? `\n${e.error}` : ""}`}
                    className={cx(
                      "flex h-11 w-full items-center gap-2.5 rounded-lg px-2.5 text-left",
                      openable ? "hover:bg-hover" : "cursor-default opacity-80",
                    )}
                  >
                    <MethodPill method={e.method} className="min-w-10 px-[7px] py-[3px] text-[10.5px]" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[14px]">{e.name}</span>
                    </span>
                    {e.testsFailed > 0 && <span className="text-[11px] text-danger">{e.testsFailed} failed</span>}
                    <span className={cx("font-mono text-[11.5px]", e.status ? statusColor(e.status) : "text-danger")}>{e.status ?? "ERR"}</span>
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
