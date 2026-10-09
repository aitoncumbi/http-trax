"use client";

import { Bot, History, Settings } from "lucide-react";

import { useStore, type View } from "@/lib/store";
import { cx } from "@/lib/util";

function CollectionsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <rect x="2.5" y="4.5" width="8.5" height="9" rx="1.8" />
      <path d="M5 4.5V3.8A1.3 1.3 0 0 1 6.3 2.5h5.9A1.3 1.3 0 0 1 13.5 3.8v6.9a1.3 1.3 0 0 1-1.3 1.3H11" />
    </svg>
  );
}

function EnvironmentsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden>
      <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
    </svg>
  );
}

function BranchIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden>
      <circle cx="4.5" cy="3.5" r="1.5" />
      <circle cx="4.5" cy="12.5" r="1.5" />
      <circle cx="11.5" cy="6" r="1.5" />
      <path d="M4.5 5v6M11.5 7.5c0 2-2 2.5-5.5 3.5" />
    </svg>
  );
}

export function ActivityBar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const changes = useStore((s) => (s.activeWorkspaceId ? (s.sync[s.activeWorkspaceId]?.changes ?? 0) : 0));
  const items: { id: View; label: string; icon: React.ReactNode; badge?: number }[] = [
    { id: "collections", label: "Collections", icon: <CollectionsIcon /> },
    { id: "environments", label: "Environments", icon: <EnvironmentsIcon /> },
    { id: "history", label: "History", icon: <History size={18} strokeWidth={1.4} /> },
    { id: "git", label: changes ? `Source control, ${changes} changes` : "Source control", icon: <BranchIcon />, badge: changes },
    { id: "mcp", label: "AI agents (MCP)", icon: <Bot size={18} strokeWidth={1.4} /> },
  ];
  const button = (id: View, label: string, icon: React.ReactNode, badge?: number) => (
    <div key={id} className="relative">
      <button
        type="button"
        title={label}
        aria-label={label}
        aria-pressed={view === id}
        onClick={() => setView(id)}
        className={cx(
          "flex h-[34px] w-[34px] items-center justify-center rounded-lg transition-colors",
          view === id ? "bg-selected text-text" : "text-muted hover:bg-hover hover:text-text",
        )}
      >
        {icon}
      </button>
      {!!badge && (
        <span className="pointer-events-none absolute top-px right-px h-[15px] min-w-[15px] rounded-full bg-accent px-1 text-center text-[10px] leading-[15px] font-bold text-white">
          {badge > 99 ? "99+" : badge}
        </span>
      )}
    </div>
  );
  return (
    <nav aria-label="Sections" className="flex w-[46px] shrink-0 flex-col items-center gap-1 border-r border-border bg-rail pt-2 pb-2.5">
      {items.map((it) => button(it.id, it.label, it.icon, it.badge))}
      <div className="flex-1" />
      {button("settings", "Settings", <Settings size={18} strokeWidth={1.3} />)}
    </nav>
  );
}
