"use client";

import { useEffect, useEffectEvent, useRef, useState, type ReactNode } from "react";

import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { SyncReport, WorkspaceEntry, WorkspaceSummary } from "@/lib/types";
import { cx } from "@/lib/util";

import { WorkspaceAvatar } from "./ui/Brand";

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

function subtitle(w: WorkspaceEntry, sync: SyncReport | undefined, summary: WorkspaceSummary | undefined): string {
  if (summary && !summary.exists) return "Folder not found";
  const requests = summary ? plural(summary.requests, "request") : null;
  const local = sync?.state === "local-only" || sync?.state === "not-repo" || (summary && (!summary.isRepo || !summary.remote));
  if (local) return ["This device only", "not synced"].join(" · ");
  const parts = [w.team ? "Team" : "Shared"];
  if (summary?.members && summary.members > 1) parts.push(plural(summary.members, "member"));
  if (requests) parts.push(requests);
  if (sync?.state === "offline") parts.push("offline");
  if (sync?.state === "auth-required") parts.push("sign-in needed");
  return parts.join(" · ");
}

/** Dropdown under the workspace button (design: "Workspace switcher"). */
export function WorkspaceSwitcher({ onClose }: { onClose: () => void }) {
  const workspaces = useStore((s) => s.workspaces);
  const activeId = useStore((s) => s.activeWorkspaceId);
  const sync = useStore((s) => s.sync);
  const { selectWorkspace, showWorkspaceDialog, showImportDialog } = useStore.getState();
  const ref = useRef<HTMLDivElement>(null);
  const [summaries, setSummaries] = useState<Record<string, WorkspaceSummary>>({});
  const close = useEffectEvent(() => onClose());

  useEffect(() => {
    let cancelled = false;
    api
      .workspaceSummaries()
      .then((list) => !cancelled && setSummaries(Object.fromEntries(list.map((s) => [s.id, s]))))
      .catch(() => {});
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    // Defer so the click that opened us doesn't close us.
    const t = setTimeout(() => window.addEventListener("mousedown", onDown), 0);
    window.addEventListener("keydown", onKey);
    return () => {
      cancelled = true;
      clearTimeout(t);
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const action = (label: string, icon: ReactNode, run: () => void) => (
    <button
      type="button"
      role="menuitem"
      onClick={() => {
        onClose();
        run();
      }}
      className="flex h-[38px] w-full items-center gap-3 rounded-[9px] px-3 text-left text-[14px] text-text-2 hover:bg-hover"
    >
      <span className="text-muted">{icon}</span>
      {label}
    </button>
  );
  const svg = (children: ReactNode) => (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      {children}
    </svg>
  );

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Workspaces"
      className="absolute top-[38px] left-0 z-50 w-[352px] rounded-[14px] border border-border-strong bg-elev p-2 shadow-[var(--shadow)]"
    >
      <div className="px-2.5 py-2 text-[11.5px] font-semibold tracking-[0.04em] text-faint">Workspaces</div>
      <div className="max-h-[360px] overflow-y-auto">
        {workspaces.map((w, i) => {
          const active = w.id === activeId;
          const s = sync[w.id];
          const summary = summaries[w.id];
          const local = s?.state === "local-only" || s?.state === "not-repo" || (summary !== undefined && !summary.isRepo);
          return (
            <button
              key={w.id}
              type="button"
              role="menuitemradio"
              aria-checked={active}
              onClick={() => {
                onClose();
                if (!active) void selectWorkspace(w.id);
              }}
              className={cx("relative flex h-14 w-full items-center gap-3 rounded-[9px] px-3 text-left", active ? "bg-selected" : "hover:bg-hover")}
            >
              <WorkspaceAvatar id={w.id} name={w.name} size={34} local={local && !w.team} />
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className={cx("truncate text-[14px] leading-[18px]", active ? "font-semibold" : "font-medium text-text-2")}>{w.name}</span>
                <span className="truncate text-[12px] leading-[15px] text-faint">{subtitle(w, s, summary)}</span>
              </span>
              {active ? (
                <span className="inline-flex h-[18px] w-[18px] items-center justify-center rounded-full bg-accent">
                  <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="#ffffff" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                    <path d="m3.5 8.3 3 3 6-6.6" />
                  </svg>
                </span>
              ) : (
                i < 9 && <kbd className="kbd">Ctrl {i + 1}</kbd>
              )}
            </button>
          );
        })}
      </div>
      <div className="mx-1.5 my-2 h-px bg-border" />
      {action("New workspace", svg(<path d="M8 3.5v9M3.5 8h9" />), () => showWorkspaceDialog("create"))}
      {action(
        "Join a team repository",
        svg(<path d="M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2-2a2.6 2.6 0 0 0-3.7-3.7l-.8.8M9.2 6.8a2.6 2.6 0 0 0-3.7 0l-2 2a2.6 2.6 0 0 0 3.7 3.7l.8-.8" />),
        () => showWorkspaceDialog("clone"),
      )}
      {action("Join with an onboarding link", svg(<path d="M3 8h7.5M7.5 5 10.5 8l-3 3M13 3v10" />), () => showWorkspaceDialog("join"))}
      {action("Open a folder", svg(<path d="M2 4.4A1.5 1.5 0 0 1 3.5 2.9h2.4L7.2 4.4h5.3a1.5 1.5 0 0 1 1.5 1.5v5.8a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.7z" />), () =>
        showWorkspaceDialog("open"),
      )}
      {action("Import from another client", svg(<path d="M8 11V3.5M5 6.5l3-3 3 3M3.5 12.5h9" />), () => showImportDialog("postman"))}
    </div>
  );
}
