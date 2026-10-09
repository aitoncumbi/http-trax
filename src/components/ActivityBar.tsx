"use client";

import { Bot, FolderTree, GitBranch, Settings, SlidersHorizontal } from "lucide-react";

import { useStore, type View } from "@/lib/store";
import { cx } from "@/lib/util";

export function ActivityBar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const changes = useStore((s) => (s.activeWorkspaceId ? (s.sync[s.activeWorkspaceId]?.changes ?? 0) : 0));
  const items: { id: View; label: string; icon: React.ReactNode; badge?: number }[] = [
    { id: "collections", label: "Collections", icon: <FolderTree size={19} /> },
    { id: "environments", label: "Environments", icon: <SlidersHorizontal size={19} /> },
    { id: "git", label: "Source control", icon: <GitBranch size={19} />, badge: changes },
    { id: "mcp", label: "AI agents (MCP)", icon: <Bot size={19} /> },
  ];
  return (
    <nav className="flex w-12 shrink-0 flex-col items-center gap-1 border-r border-border bg-panel py-2" aria-label="Views">
      {items.map((it) => (
        <button
          key={it.id}
          title={it.label}
          aria-label={it.label}
          aria-current={view === it.id}
          onClick={() => setView(it.id)}
          className={cx(
            "relative flex h-10 w-10 items-center justify-center rounded-lg transition-colors",
            view === it.id ? "bg-accent-soft text-accent" : "text-faint hover:bg-hover hover:text-text",
          )}
        >
          {it.icon}
          {!!it.badge && (
            <span className="absolute top-1 right-1 min-w-4 rounded-full bg-info px-1 text-center text-[9.5px] leading-4 font-bold text-white">
              {it.badge > 99 ? "99+" : it.badge}
            </span>
          )}
        </button>
      ))}
      <div className="flex-1" />
      <button
        title="Workspace settings"
        aria-label="Workspace settings"
        onClick={() => setView("settings")}
        className={cx(
          "flex h-10 w-10 items-center justify-center rounded-lg",
          view === "settings" ? "bg-accent-soft text-accent" : "text-faint hover:bg-hover hover:text-text",
        )}
      >
        <Settings size={19} />
      </button>
    </nav>
  );
}
