"use client";

import { Check, FolderOpen, Link2, Plus } from "lucide-react";
import { useEffect, useRef } from "react";

import { useStore } from "@/lib/store";
import type { SyncReport, WorkspaceEntry } from "@/lib/types";
import { cx } from "@/lib/util";

import { WorkspaceAvatar } from "./ui/Brand";

function subtitle(w: WorkspaceEntry, sync: SyncReport | undefined): string {
  const where = (() => {
    switch (sync?.state) {
      case "local-only":
      case "not-repo":
        return "This device only · not synced";
      case "offline":
        return "Offline";
      case "auth-required":
        return "Sign-in needed";
      case undefined:
        return w.path.split(/[\\/]/).slice(-2).join("/");
      default:
        return sync?.upstream ? `Synced with ${sync.upstream.split("/")[0]}` : "Git repository";
    }
  })();
  return w.team ? `${w.team} · ${where}` : where;
}

/** Dropdown under the workspace button (design: "Workspace switcher"). */
export function WorkspaceSwitcher({ onClose }: { onClose: () => void }) {
  const workspaces = useStore((s) => s.workspaces);
  const activeId = useStore((s) => s.activeWorkspaceId);
  const sync = useStore((s) => s.sync);
  const { selectWorkspace, showWorkspaceDialog } = useStore.getState();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    // Defer so the click that opened us doesn't close us.
    const t = setTimeout(() => window.addEventListener("mousedown", onDown), 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const action = (label: string, icon: React.ReactNode, run: () => void) => (
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

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Workspaces"
      className="absolute top-[34px] left-0 z-50 w-[352px] rounded-[14px] border border-border-strong bg-elev p-2 shadow-[var(--shadow)]"
    >
      <div className="px-2.5 py-2 text-[11.5px] font-semibold tracking-[0.04em] text-faint">Workspaces</div>
      <div className="max-h-[360px] overflow-y-auto">
        {workspaces.map((w, i) => {
          const active = w.id === activeId;
          const s = sync[w.id];
          const local = s?.state === "local-only" || s?.state === "not-repo";
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
              className={cx(
                "relative flex h-14 w-full items-center gap-3 rounded-[9px] px-3 text-left",
                active ? "bg-selected" : "hover:bg-hover",
              )}
            >
              {active && <span className="absolute top-0 bottom-0 left-0 w-[3px] rounded-sm bg-accent-line" />}
              <WorkspaceAvatar id={w.id} name={w.name} size={34} local={local && !w.team} />
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className={cx("truncate text-[14px] leading-[18px]", active ? "font-semibold" : "font-medium text-text-2")}>{w.name}</span>
                <span className="truncate text-[12px] leading-[15px] text-faint">{subtitle(w, s)}</span>
              </span>
              {active ? (
                <span className="inline-flex h-[18px] w-[18px] items-center justify-center rounded-full bg-accent">
                  <Check size={11} strokeWidth={3} className="text-white" />
                </span>
              ) : (
                i < 9 && <kbd className="kbd">Ctrl {i + 1}</kbd>
              )}
            </button>
          );
        })}
      </div>
      <div className="mx-1.5 my-2 h-px bg-border" />
      {action("New workspace", <Plus size={16} strokeWidth={1.5} />, () => showWorkspaceDialog("create"))}
      {action("Clone a team repository", <Link2 size={16} strokeWidth={1.5} />, () => showWorkspaceDialog("clone"))}
      {action("Open a folder", <FolderOpen size={16} strokeWidth={1.5} />, () => showWorkspaceDialog("open"))}
    </div>
  );
}
