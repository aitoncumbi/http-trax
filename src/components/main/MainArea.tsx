"use client";

import { FileText, GitCompare, GitPullRequest, Play, Plus, Settings2, X } from "lucide-react";
import { useEffect, useRef } from "react";

import { tabDirty, useStore, useWorkspaceTabs, type Tab } from "@/lib/store";
import { cx, scrollSideways } from "@/lib/util";

import { useContextMenu } from "../ui/Menu";
import { DiffView } from "./DiffView";
import { GroupEditor } from "./GroupEditor";
import { RequestView } from "./RequestView";
import { ReviewView } from "./ReviewView";
import { RunnerView } from "./RunnerView";
import { Welcome } from "./Welcome";

function RequestIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M3 5.5h9.5M10 3l2.5 2.5L10 8M13 10.5H3.5M6 8l-2.5 2.5L6 13" />
    </svg>
  );
}

function tabTitle(tab: Tab): { icon: React.ReactNode; title: string; hint: string } {
  switch (tab.kind) {
    case "request":
      return { icon: <RequestIcon />, title: tab.draft.name || "Untitled", hint: `${tab.draft.method} ${tab.draft.url}` };
    case "group":
      return { icon: <Settings2 size={15} strokeWidth={1.4} />, title: tab.draft.name, hint: "Collection settings" };
    case "runner":
      return { icon: <Play size={15} strokeWidth={1.4} />, title: `Run ${tab.groupName}`, hint: "Collection run" };
    case "review":
      return { icon: <GitPullRequest size={15} strokeWidth={1.4} />, title: `${tab.head} → ${tab.base}`, hint: "Review" };
    case "diff":
      return { icon: <GitCompare size={15} strokeWidth={1.4} />, title: tab.title, hint: tab.title };
  }
}

function TabButton({ tab, active }: { tab: Tab; active: boolean }) {
  const { focusTab, closeTab } = useStore.getState();
  const dirty = tabDirty(tab);
  const { icon, title, hint } = tabTitle(tab);
  const ref = useRef<HTMLDivElement>(null);
  // The strip scrolls without a scrollbar, so bring the active tab into view.
  useEffect(() => {
    if (active) ref.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active]);
  const ctx = useContextMenu(() => [
    { label: "Close", hint: "Ctrl+W", onSelect: () => void closeTab(tab.id) },
    {
      label: "Close others",
      onSelect: () => {
        for (const t of useStore.getState().tabs) {
          if (t.workspaceId === tab.workspaceId && t.id !== tab.id && !tabDirty(t)) void closeTab(t.id);
        }
      },
    },
  ]);
  return (
    <div
      ref={ref}
      role="tab"
      aria-selected={active}
      title={hint}
      onMouseDown={(e) => e.button === 1 && (e.preventDefault(), void closeTab(tab.id))}
      onContextMenu={ctx.open}
      onClick={() => focusTab(tab.id)}
      className={cx(
        "group flex max-w-[240px] shrink-0 cursor-pointer items-center gap-2 border-b-2 pr-1.5 pl-3 text-[14px]",
        active ? "border-accent-line font-medium text-text" : "border-transparent text-muted hover:text-text",
      )}
    >
      <span className={cx("shrink-0", tab.kind === "runner" && "text-accent-text", tab.kind === "review" && "text-info")}>{icon}</span>
      <span className="truncate">{title}</span>
      <span className="relative flex h-5 w-5 shrink-0 items-center justify-center">
        {dirty && <span className="h-[7px] w-[7px] rounded-full bg-accent-line group-hover:hidden" aria-label="Unsaved changes" />}
        <button
          type="button"
          aria-label={`Close ${title}`}
          onClick={(e) => {
            e.stopPropagation();
            void closeTab(tab.id);
          }}
          className={cx(
            "absolute inset-0 hidden items-center justify-center rounded text-faint hover:bg-hover hover:text-text group-hover:flex",
            !dirty && active && "flex",
          )}
        >
          <X size={13} strokeWidth={1.6} />
        </button>
      </span>
      {ctx.menu}
    </div>
  );
}

export function MainArea() {
  const { tabs, active } = useWorkspaceTabs();
  const newRequestHere = useStore((s) => s.newRequestHere);
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      {tabs.length > 0 && (
        <div role="tablist" aria-label="Open tabs" onWheel={scrollSideways} className="tab-strip tab-scroll flex h-9 shrink-0 items-stretch gap-0.5 px-1">
          {tabs.map((t) => (
            <TabButton key={t.id} tab={t} active={t.id === active?.id} />
          ))}
          <button type="button" aria-label="New request" title="New request (Ctrl+N)" onClick={() => void newRequestHere()} className="icon-btn shrink-0 self-center">
            <Plus size={15} strokeWidth={1.6} />
          </button>
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col">
        {!active && <Welcome />}
        {active?.kind === "request" && <RequestView key={active.id} tab={active} />}
        {active?.kind === "group" && <GroupEditor key={active.id} tab={active} />}
        {active?.kind === "runner" && <RunnerView key={active.id} tab={active} />}
        {active?.kind === "review" && <ReviewView key={active.id} tab={active} />}
        {active?.kind === "diff" && (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex h-10 items-center gap-2 border-b border-border px-4 text-[13px] text-muted">
              <FileText size={14} strokeWidth={1.4} /> {active.title}
            </div>
            <DiffView key={active.id} diff={active.text} />
          </div>
        )}
      </div>
    </div>
  );
}
