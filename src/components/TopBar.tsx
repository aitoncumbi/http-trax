"use client";

import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Check,
  ChevronDown,
  CloudOff,
  FolderOpen,
  GitBranch,
  KeyRound,
  Layers,
  Loader2,
  Moon,
  Plus,
  RefreshCw,
  Sun,
  Trash2,
} from "lucide-react";

import { useStore } from "@/lib/store";
import type { SyncReport } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

import { MenuButton, type MenuItem } from "./ui/Menu";

function syncLabel(r: SyncReport | undefined): { text: string; tone: string; icon: React.ReactNode } {
  if (!r) return { text: "Not synced yet", tone: "text-faint", icon: <RefreshCw size={13} /> };
  switch (r.state) {
    case "synced":
      return { text: "Up to date", tone: "text-ok", icon: <Check size={13} /> };
    case "ahead":
      return { text: `${r.ahead} to push`, tone: "text-info", icon: <ArrowUp size={13} /> };
    case "behind":
      return { text: `${r.behind} to pull`, tone: "text-warn", icon: <ArrowDown size={13} /> };
    case "diverged":
      return { text: `↑${r.ahead} ↓${r.behind} diverged`, tone: "text-warn", icon: <GitBranch size={13} /> };
    case "conflicts":
      return { text: "Merge conflicts", tone: "text-danger", icon: <AlertTriangle size={13} /> };
    case "no-upstream":
      return { text: "Not published", tone: "text-info", icon: <ArrowUp size={13} /> };
    case "local-only":
      return { text: "Local only", tone: "text-faint", icon: <FolderOpen size={13} /> };
    case "not-repo":
      return { text: "No git", tone: "text-faint", icon: <FolderOpen size={13} /> };
    case "offline":
      return { text: "Offline", tone: "text-faint", icon: <CloudOff size={13} /> };
    case "auth-required":
      return { text: "Sign-in needed", tone: "text-warn", icon: <KeyRound size={13} /> };
    default:
      return { text: "Sync error", tone: "text-danger", icon: <AlertTriangle size={13} /> };
  }
}

export function TopBar() {
  const workspaces = useStore((s) => s.workspaces);
  const activeId = useStore((s) => s.activeWorkspaceId);
  const tree = useStore((s) => s.tree);
  const sync = useStore((s) => (s.activeWorkspaceId ? s.sync[s.activeWorkspaceId] : undefined));
  const syncing = useStore((s) => s.syncing);
  const theme = useStore((s) => s.theme);
  const { selectWorkspace, showWorkspaceDialog, setActiveEnvironment, syncNow, toggleTheme, openEnvironment, createEnvironment } =
    useStore.getState();
  const entry = workspaces.find((w) => w.id === activeId) ?? null;
  const activeEnvId = entry?.prefs.activeEnvironment ?? null;
  const activeEnv = tree?.environments.find((e) => e.id === activeEnvId) ?? null;

  // Group workspaces by team for the switcher.
  const teams = new Map<string, typeof workspaces>();
  for (const w of workspaces) {
    const team = w.team || "Personal";
    teams.set(team, [...(teams.get(team) ?? []), w]);
  }
  const wsItems: MenuItem[] = [];
  for (const [team, list] of teams) {
    if (wsItems.length) wsItems.push({ label: "", separator: true });
    wsItems.push({ label: team.toUpperCase(), disabled: true });
    for (const w of list) {
      wsItems.push({
        label: w.name,
        icon: w.id === activeId ? <Check size={13} className="text-accent" /> : <Layers size={13} />,
        hint: w.path.split(/[\\/]/).slice(-2).join("/"),
        onSelect: () => void selectWorkspace(w.id),
      });
    }
  }
  wsItems.push(
    { label: "", separator: true },
    { label: "New workspace…", icon: <Plus size={13} />, onSelect: () => showWorkspaceDialog("create") },
    { label: "Open folder…", icon: <FolderOpen size={13} />, onSelect: () => showWorkspaceDialog("open") },
    { label: "Clone team repository…", icon: <GitBranch size={13} />, onSelect: () => showWorkspaceDialog("clone") },
  );
  if (entry) {
    wsItems.push({
      label: `Remove "${entry.name}" from list`,
      icon: <Trash2 size={13} />,
      danger: true,
      onSelect: async () => {
        const ok = await useStore.getState().confirm({
          title: "Remove workspace from the list?",
          message: "The files on disk are not touched. You can open the folder again later.",
          confirm: "Remove",
        });
        if (ok) {
          const { api } = await import("@/lib/api");
          await api.removeWorkspace(entry.id);
          await useStore.getState().refreshWorkspaces();
        }
      },
    });
  }

  const envItems: MenuItem[] = [
    {
      label: "No environment",
      icon: activeEnvId ? undefined : <Check size={13} className="text-accent" />,
      onSelect: () => void setActiveEnvironment(null),
    },
    ...(tree?.environments ?? []).map((e) => ({
      label: e.name,
      icon: e.id === activeEnvId ? <Check size={13} className="text-accent" /> : undefined,
      hint: `${e.variables.length} vars`,
      onSelect: () => void setActiveEnvironment(e.id),
    })),
    { label: "", separator: true },
    ...(activeEnv ? [{ label: `Edit "${activeEnv.name}"`, onSelect: () => openEnvironment(activeEnv.id) }] : []),
    { label: "New environment…", icon: <Plus size={13} />, onSelect: () => void createEnvironment() },
  ];

  const s = syncLabel(sync);

  return (
    <header className="flex h-11 shrink-0 items-center gap-2 border-b border-border bg-panel px-2.5" data-tauri-drag-region>
      <div className="flex items-center gap-2 pr-2">
        <svg width="20" height="20" viewBox="0 0 32 32" aria-hidden>
          <rect width="32" height="32" rx="8" fill="var(--elev)" />
          <path d="M8 12.5h13M17.5 8.5l4.5 4-4.5 4" stroke="var(--accent)" strokeWidth="2.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M24 19.5H11M14.5 15.5l-4.5 4 4.5 4" stroke="var(--muted)" strokeWidth="2.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span className="text-[13px] font-semibold tracking-tight">http-trax</span>
      </div>

      <MenuButton items={wsItems} minWidth={300} className="flex h-8 items-center gap-2 rounded-md border border-border bg-panel-2 px-2.5 hover:bg-hover" title="Switch workspace">
        <Layers size={14} className="text-muted" />
        <span className="max-w-[220px] truncate font-medium">{entry?.name ?? "No workspace"}</span>
        {entry?.team && <span className="rounded bg-accent-soft px-1.5 text-[10.5px] font-semibold text-accent">{entry.team}</span>}
        <ChevronDown size={13} className="text-faint" />
      </MenuButton>

      <MenuButton items={envItems} minWidth={240} className="flex h-8 items-center gap-2 rounded-md border border-border bg-panel-2 px-2.5 hover:bg-hover" title="Active environment">
        <span className={cx("h-2 w-2 rounded-full", activeEnv ? "bg-accent" : "bg-faint")} />
        <span className="max-w-[160px] truncate">{activeEnv?.name ?? "No environment"}</span>
        <ChevronDown size={13} className="text-faint" />
      </MenuButton>

      <div className="flex-1" data-tauri-drag-region />

      {entry && (
        <button
          onClick={() => void syncNow()}
          title={
            sync
              ? `${sync.branch ?? ""}${sync.upstream ? ` → ${sync.upstream}` : ""}\nLast fetch: ${timeAgo(sync.fetchedAt)}${sync.message ? `\n${sync.message}` : ""}\nClick to sync now`
              : "Sync now"
          }
          className="flex h-8 items-center gap-2 rounded-md px-2.5 text-[12px] hover:bg-hover"
        >
          <span className={cx("flex items-center gap-1.5", s.tone)}>
            {syncing ? <Loader2 size={13} className="animate-spin" /> : s.icon}
            {s.text}
          </span>
          {sync?.branch && (
            <span className="flex items-center gap-1 text-faint">
              <GitBranch size={12} />
              {sync.branch}
            </span>
          )}
          {sync?.fetchedAt && <span className="text-faint">· {timeAgo(sync.fetchedAt)}</span>}
        </button>
      )}

      <button className="icon-btn" onClick={toggleTheme} title="Toggle theme" aria-label="Toggle theme">
        {theme === "dark" ? <Sun size={15} /> : <Moon size={15} />}
      </button>
    </header>
  );
}
