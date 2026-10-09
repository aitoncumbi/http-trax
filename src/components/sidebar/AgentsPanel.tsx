"use client";

import { useStore } from "@/lib/store";
import { cx } from "@/lib/util";

import { CLIENT_MONO, statusLabel, useAgents } from "../agents/agentsStore";
import { SidebarIconButton, SidebarSection } from "./Sidebar";

/** Sidebar of the AI agents view: MCP clients and their state. */
export function AgentsPanel() {
  const overview = useAgents((s) => s.overview);
  const selected = useAgents((s) => s.client);
  const setClient = useAgents((s) => s.setClient);
  const changes = useStore((s) => (s.activeWorkspaceId ? (s.sync[s.activeWorkspaceId]?.changes ?? 0) : 0));
  const setView = useStore((s) => s.setView);
  const clients = overview?.clients ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <SidebarSection
        title="Clients"
        actions={
          <SidebarIconButton label="Set up a client" onClick={() => setClient(clients.find((c) => c.status === "not-set-up")?.id ?? "code")}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden>
              <path d="M8 3.5v9M3.5 8h9" />
            </svg>
          </SidebarIconButton>
        }
      />
      <div className="flex flex-col gap-px">
        {clients.map((c, i) => (
          <button
            key={`${c.id}-${i}`}
            type="button"
            aria-pressed={c.id === selected}
            onClick={() => c.id !== "other" && setClient(c.id)}
            title={c.session ? `${c.session.client}${c.session.version ? ` ${c.session.version}` : ""}` : (c.configPath ?? undefined)}
            className={cx("flex h-[34px] items-center gap-2.5 rounded-[7px] px-2 text-left text-[13.5px]", c.id === selected ? "bg-selected" : "hover:bg-hover")}
          >
            <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-[5px] bg-border-soft text-[10px] font-bold text-muted" aria-hidden>
              {CLIENT_MONO[c.id]}
            </span>
            <span className="min-w-0 flex-1 truncate">{c.id === "desktop" ? (c.session?.client ?? "Claude Desktop") : c.name}</span>
            <span className="inline-flex items-center gap-1.5 text-[11.5px] text-faint">
              <span className={cx("h-1.5 w-1.5 rounded-full", c.status === "connected" ? "bg-ok" : c.status === "configured" ? "bg-accent-text" : "bg-checkbox-off")} aria-hidden />
              {statusLabel(c)}
            </span>
          </button>
        ))}
        {!overview && <p className="px-2 py-2 text-[12.5px] text-faint">Loading…</p>}
      </div>
      {changes > 0 && (
        <div className="mx-0.5 mt-3.5 rounded-[10px] border border-border p-3">
          <div className="text-[13px] font-medium">
            {changes} {changes === 1 ? "change" : "changes"} to review
          </div>
          <div className="mt-[3px] text-[12.5px] leading-[18px] text-faint">Agent edits land as uncommitted changes in this workspace.</div>
          <button type="button" className="btn mt-2.5 h-7 px-2.5" onClick={() => setView("git")}>
            Review changes
          </button>
        </div>
      )}
    </div>
  );
}
