"use client";

import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Check,
  ChevronDown,
  CloudOff,
  GitBranch,
  KeyRound,
  Loader2,
  Minus,
  Plus,
  Search,
  Square,
  SquareTerminal,
  X,
} from "lucide-react";
import { useState } from "react";

import { isTauri, windowAction } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { SyncReport } from "@/lib/types";
import { cx, envTone, timeAgo } from "@/lib/util";

import { CourionMark, WorkspaceAvatar } from "./ui/Brand";
import { MenuButton, type MenuItem } from "./ui/Menu";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

function syncSummary(r: SyncReport | undefined): { text: string; tone: string; icon: React.ReactNode } | null {
  if (!r) return null;
  switch (r.state) {
    case "synced":
      return { text: "Up to date", tone: "text-faint", icon: <Check size={12} strokeWidth={1.8} /> };
    case "ahead":
    case "no-upstream":
      return { text: r.state === "ahead" ? `${r.ahead} to push` : "Not published", tone: "text-accent-text font-medium", icon: <ArrowUp size={12} strokeWidth={1.8} /> };
    case "behind":
      return { text: `${r.behind} to pull`, tone: "text-warn font-medium", icon: <ArrowDown size={12} strokeWidth={1.8} /> };
    case "diverged":
      return { text: `${r.ahead} to push · ${r.behind} to pull`, tone: "text-warn font-medium", icon: <GitBranch size={12} /> };
    case "conflicts":
      return { text: "Merge conflicts", tone: "text-danger font-medium", icon: <AlertTriangle size={12} /> };
    case "offline":
      return { text: "Offline", tone: "text-faint", icon: <CloudOff size={12} /> };
    case "auth-required":
      return { text: "Sign-in needed", tone: "text-warn", icon: <KeyRound size={12} /> };
    case "error":
      return { text: "Sync error", tone: "text-danger", icon: <AlertTriangle size={12} /> };
    default:
      return null;
  }
}

function ToolbarButton({ children, ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "inline-flex h-7 items-center gap-1.5 rounded-[7px] px-2.5 text-[13px] leading-none text-text-2 hover:bg-hover hover:text-text",
        props.className,
      )}
    >
      {children}
    </button>
  );
}

export function TopBar() {
  const workspaces = useStore((s) => s.workspaces);
  const activeId = useStore((s) => s.activeWorkspaceId);
  const tree = useStore((s) => s.tree);
  const sync = useStore((s) => (s.activeWorkspaceId ? s.sync[s.activeWorkspaceId] : undefined));
  const syncing = useStore((s) => s.syncing);
  const { setActiveEnvironment, openEnvironment, createEnvironment, newRequestHere, showImportDialog, openPalette, setView, syncNow } =
    useStore.getState();
  const [switcher, setSwitcher] = useState(false);
  const entry = workspaces.find((w) => w.id === activeId) ?? null;
  const activeEnvId = entry?.prefs.activeEnvironment ?? null;
  const activeEnv = tree?.environments.find((e) => e.id === activeEnvId) ?? null;
  const summary = syncSummary(sync);
  const desktop = isTauri();

  const envItems: MenuItem[] = [
    {
      label: "No environment",
      icon: <span className="h-2 w-2 rounded-full" style={{ background: envTone(null) }} />,
      hint: activeEnvId ? undefined : "Active",
      onSelect: () => void setActiveEnvironment(null),
    },
    ...(tree?.environments ?? []).map((e) => ({
      label: e.name,
      icon: <span className="h-2 w-2 rounded-full" style={{ background: envTone(e.name) }} />,
      hint: e.id === activeEnvId ? "Active" : undefined,
      onSelect: () => void setActiveEnvironment(e.id),
    })),
    { label: "", separator: true },
    ...(activeEnv ? [{ label: `Edit ${activeEnv.name}`, onSelect: () => openEnvironment(activeEnv.id) }] : []),
    { label: "New environment…", icon: <Plus size={14} />, onSelect: () => void createEnvironment() },
  ];

  return (
    <header
      className="relative flex h-10 shrink-0 items-center gap-0.5 border-b border-border bg-bg pl-3 select-none"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-2 pr-1.5" data-tauri-drag-region>
        <CourionMark />
        <span className="text-[14px] font-bold tracking-[-0.02em]" data-tauri-drag-region>
          courion
        </span>
        {!desktop && (
          <span className="rounded border border-border-strong px-1.5 text-[11px] leading-[18px] text-faint" title="Running in a browser with sample data. Install the desktop app to send real requests.">
            Preview
          </span>
        )}
      </div>

      <div className="relative">
        <button
          type="button"
          aria-label={`Workspace: ${entry?.name ?? "none"}`}
          aria-expanded={switcher}
          onClick={() => setSwitcher((v) => !v)}
          className="inline-flex h-7 items-center gap-[7px] rounded-[7px] bg-panel-2 pr-1.5 pl-1 text-[13px] font-medium hover:bg-selected"
        >
          {entry ? <WorkspaceAvatar id={entry.id} name={entry.name} local={!entry.team && sync?.state === "local-only"} /> : null}
          <span className="max-w-[200px] truncate leading-none">{entry?.name ?? "No workspace"}</span>
          <ChevronDown size={11} strokeWidth={2} className="text-faint" />
        </button>
        {switcher && <WorkspaceSwitcher onClose={() => setSwitcher(false)} />}
      </div>

      <span className="mx-1.5 h-3.5 w-px bg-sep" aria-hidden />

      <ToolbarButton onClick={() => void newRequestHere()} title="New request (Ctrl+N)" disabled={!entry}>
        <Plus size={14} strokeWidth={1.8} />
        New
      </ToolbarButton>
      <ToolbarButton onClick={showImportDialog} title="Import from cURL (Ctrl+Shift+I)" disabled={!entry}>
        <SquareTerminal size={14} strokeWidth={1.6} />
        Import
      </ToolbarButton>
      <MenuButton
        items={envItems}
        minWidth={240}
        title="Environment (Ctrl+E)"
        className="inline-flex h-7 items-center gap-1.5 rounded-[7px] px-2.5 text-[13px] leading-none text-text-2 hover:bg-hover hover:text-text"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden>
          <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
        </svg>
        <span>{activeEnv?.name ?? "No environment"}</span>
        <span className="h-[7px] w-[7px] rounded-full" style={{ background: envTone(activeEnv?.name) }} aria-hidden />
      </MenuButton>

      <div className="h-full flex-1" data-tauri-drag-region />

      {entry && (sync?.branch || summary) && (
        <button
          type="button"
          onClick={() => setView("git")}
          onDoubleClick={() => void syncNow()}
          title={
            sync
              ? `${sync.branch ?? ""}${sync.upstream ? ` → ${sync.upstream}` : ""}\nLast fetch: ${timeAgo(sync.fetchedAt)}${sync.message ? `\n${sync.message}` : ""}\nClick for source control · double-click to sync now`
              : "Source control"
          }
          className="mr-2.5 flex h-7 items-center gap-3 rounded-[7px] px-1.5 text-[12px] leading-none whitespace-nowrap text-faint hover:bg-hover"
        >
          {syncing ? (
            <span className="flex items-center gap-1.5">
              <Loader2 size={12} className="animate-spin" /> Syncing
            </span>
          ) : (
            summary && (
              <span className={cx("flex items-center gap-[5px]", summary.tone)}>
                {summary.icon}
                {summary.text}
              </span>
            )
          )}
          {sync?.branch && (
            <span className="flex items-center gap-[5px]">
              <GitBranch size={12} strokeWidth={1.5} />
              {sync.branch}
            </span>
          )}
        </button>
      )}

      <button
        type="button"
        aria-label="Search or run a command"
        onClick={() => openPalette()}
        className="inline-flex h-7 items-center gap-2 rounded-[7px] border border-border-strong pr-[5px] pl-2.5 text-[13px] leading-none text-faint hover:text-muted"
      >
        <Search size={13} strokeWidth={1.6} />
        <span>Search</span>
        <kbd className="rounded bg-panel-2 px-1.5 py-[3px] text-[10.5px] leading-none text-muted">Ctrl K</kbd>
      </button>

      {desktop ? (
        <div className="ml-1 flex h-10 items-stretch">
          <button type="button" aria-label="Minimize" onClick={() => void windowAction("minimize")} className="flex w-[38px] items-center justify-center text-muted hover:bg-hover">
            <Minus size={13} strokeWidth={1.3} />
          </button>
          <button type="button" aria-label="Maximize" onClick={() => void windowAction("toggleMaximize")} className="flex w-[38px] items-center justify-center text-muted hover:bg-hover">
            <Square size={11} strokeWidth={1.4} />
          </button>
          <button type="button" aria-label="Close" onClick={() => void windowAction("close")} className="flex w-[38px] items-center justify-center text-muted hover:bg-[#c42b1c] hover:text-white">
            <X size={14} strokeWidth={1.3} />
          </button>
        </div>
      ) : (
        <span className="w-2.5" />
      )}
    </header>
  );
}
