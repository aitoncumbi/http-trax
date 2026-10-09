"use client";

import type { ReactNode } from "react";

import { useStore, type View } from "@/lib/store";
import type { SyncReport } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

const icon = (children: ReactNode, strokeWidth = 1.4) => (
  <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    {children}
  </svg>
);

const ICONS: Record<View, ReactNode> = {
  collections: icon(
    <>
      <rect x="2.5" y="4.5" width="8.5" height="9" rx="1.8" />
      <path d="M5 4.5V3.8A1.3 1.3 0 0 1 6.3 2.5h5.9A1.3 1.3 0 0 1 13.5 3.8v6.9a1.3 1.3 0 0 1-1.3 1.3H11" />
    </>,
    1.3,
  ),
  environments: icon(<path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />),
  history: icon(<path d="M2.6 8a5.4 5.4 0 1 0 1.6-3.8M2.6 2.8v2.5h2.5M8 5v3.2l2 1.2" />),
  mcp: icon(<path d="M7 2.5l1.3 3.2L11.5 7 8.3 8.3 7 11.5 5.7 8.3 2.5 7l3.2-1.3zM12 10.5l.6 1.4 1.4.6-1.4.6-.6 1.4-.6-1.4-1.4-.6 1.4-.6z" />, 1.3),
  git: icon(
    <>
      <circle cx="4.5" cy="3.5" r="1.5" />
      <circle cx="4.5" cy="12.5" r="1.5" />
      <circle cx="11.5" cy="6" r="1.5" />
      <path d="M4.5 5v6M11.5 7.5c0 2-2 2.5-5.5 3.5" />
    </>,
  ),
  settings: icon(
    <>
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1" />
    </>,
    1.3,
  ),
};

/** One line on the sync state, for the Source control button's tooltip. */
export function syncLine(r: SyncReport | undefined): string {
  if (!r) return "";
  const branch = r.branch ? `${r.branch}${r.upstream ? ` → ${r.upstream}` : ""}` : "";
  const state = (() => {
    switch (r.state) {
      case "synced":
        return "Up to date";
      case "ahead":
        return `${r.ahead} to push`;
      case "behind":
        return `${r.behind} to pull`;
      case "diverged":
        return `${r.ahead} to push, ${r.behind} to pull`;
      case "no-upstream":
        return "Branch not published";
      case "conflicts":
        return "Merge conflicts";
      case "offline":
        return "Offline";
      case "auth-required":
        return "Sign-in needed to sync";
      case "error":
        return r.message ? `Sync error: ${r.message}` : "Sync error";
      case "local-only":
        return "No remote, this device only";
      default:
        return "";
    }
  })();
  return [branch, state, r.fetchedAt ? `fetched ${timeAgo(r.fetchedAt)}` : ""].filter(Boolean).join(" · ");
}

export function ActivityBar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const sync = useStore((s) => (s.activeWorkspaceId ? s.sync[s.activeWorkspaceId] : undefined));
  const changes = sync?.changes ?? 0;
  const syncProblem = sync && ["conflicts", "auth-required", "error"].includes(sync.state);

  const items: { id: View; label: string; badge?: number; title?: string }[] = [
    { id: "collections", label: "Collections" },
    { id: "environments", label: "Environments" },
    { id: "history", label: "History" },
    { id: "mcp", label: "AI agents" },
    {
      id: "git",
      label: changes ? `Source control, ${changes} changes` : "Source control",
      badge: changes,
      title: ["Source control", syncLine(sync)].filter(Boolean).join("\n"),
    },
  ];
  const button = ({ id, label, badge, title }: (typeof items)[number]) => (
    <div key={id} className="relative">
      <button
        type="button"
        title={title ?? label}
        aria-label={label}
        aria-pressed={view === id}
        onClick={() => setView(id)}
        className={cx(
          "flex h-[34px] w-[34px] items-center justify-center rounded-lg transition-colors",
          view === id ? "bg-selected text-text" : "text-muted hover:bg-hover hover:text-text",
        )}
      >
        {ICONS[id]}
      </button>
      {!!badge && (
        <span className="pointer-events-none absolute top-px right-px h-[15px] min-w-[15px] rounded-full bg-accent px-1 text-center text-[10px] leading-[15px] font-semibold text-white">
          {badge > 99 ? "99+" : badge}
        </span>
      )}
      {id === "git" && syncProblem && !badge && (
        <span className="pointer-events-none absolute top-1 right-1 h-2 w-2 rounded-full border-2 border-frame bg-warn" aria-hidden />
      )}
    </div>
  );
  return (
    <nav aria-label="Sections" className="flex w-12 shrink-0 flex-col items-center gap-1 bg-frame pt-2 pb-2.5">
      {items.map(button)}
      <div className="flex-1" />
      {button({ id: "settings", label: "Settings" })}
    </nav>
  );
}
