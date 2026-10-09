"use client";

import { FileText, GitCompare, GitPullRequest, Play, Settings2, SlidersHorizontal, X } from "lucide-react";

import { tabDirty, useStore, useWorkspaceTabs, type Tab } from "@/lib/store";
import { cx, methodColor } from "@/lib/util";

import { useContextMenu } from "../ui/Menu";
import { DiffView } from "./DiffView";
import { EnvironmentEditor } from "./EnvironmentEditor";
import { GroupEditor } from "./GroupEditor";
import { RequestView } from "./RequestView";
import { ReviewView } from "./ReviewView";
import { RunnerView } from "./RunnerView";
import { Welcome } from "./Welcome";

function tabTitle(tab: Tab): { icon: React.ReactNode; title: string } {
  switch (tab.kind) {
    case "request":
      return {
        icon: <span className={cx("font-mono text-[10px] font-bold", methodColor(tab.draft.method))}>{tab.draft.method}</span>,
        title: tab.draft.name || "Untitled",
      };
    case "group":
      return { icon: <Settings2 size={13} className="text-muted" />, title: tab.draft.name };
    case "environment":
      return { icon: <SlidersHorizontal size={13} className="text-muted" />, title: tab.draft.name };
    case "runner":
      return { icon: <Play size={13} className="text-accent" />, title: `Run: ${tab.groupName}` };
    case "review":
      return { icon: <GitPullRequest size={13} className="text-info" />, title: `${tab.head} → ${tab.base}` };
    case "diff":
      return { icon: <GitCompare size={13} className="text-muted" />, title: tab.title };
  }
}

function TabButton({ tab, active }: { tab: Tab; active: boolean }) {
  const { focusTab, closeTab } = useStore.getState();
  const dirty = tabDirty(tab);
  const { icon, title } = tabTitle(tab);
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
      role="tab"
      aria-selected={active}
      onMouseDown={(e) => e.button === 1 && (e.preventDefault(), void closeTab(tab.id))}
      onContextMenu={ctx.open}
      className={cx(
        "group relative flex h-9 max-w-[220px] min-w-[120px] shrink-0 cursor-pointer items-center gap-2 border-r border-border pr-1.5 pl-3",
        active ? "bg-bg text-text" : "bg-panel text-muted hover:text-text",
      )}
      onClick={() => focusTab(tab.id)}
    >
      {active && <span className="absolute inset-x-0 top-0 h-0.5 bg-accent" />}
      <span className="shrink-0">{icon}</span>
      <span className="flex-1 truncate text-[12.5px]">{title}</span>
      <button
        aria-label="Close tab"
        onClick={(e) => {
          e.stopPropagation();
          void closeTab(tab.id);
        }}
        className="flex h-5 w-5 shrink-0 items-center justify-center rounded hover:bg-hover"
      >
        {dirty ? (
          <>
            <span className="h-2 w-2 rounded-full bg-text group-hover:hidden" />
            <X size={13} className="hidden group-hover:block" />
          </>
        ) : (
          <X size={13} className={cx(!active && "opacity-0 group-hover:opacity-100")} />
        )}
      </button>
      {ctx.menu}
    </div>
  );
}

export function MainArea() {
  const { tabs, active } = useWorkspaceTabs();
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {tabs.length > 0 && (
        <div className="flex h-9 shrink-0 overflow-x-auto border-b border-border bg-panel" role="tablist">
          {tabs.map((t) => (
            <TabButton key={t.id} tab={t} active={t.id === active?.id} />
          ))}
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col">
        {!active && <Welcome />}
        {active?.kind === "request" && <RequestView key={active.id} tab={active} />}
        {active?.kind === "group" && <GroupEditor key={active.id} tab={active} />}
        {active?.kind === "environment" && <EnvironmentEditor key={active.id} tab={active} />}
        {active?.kind === "runner" && <RunnerView key={active.id} tab={active} />}
        {active?.kind === "review" && <ReviewView key={active.id} tab={active} />}
        {active?.kind === "diff" && (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-[12.5px] text-muted">
              <FileText size={14} /> {active.title}
            </div>
            <DiffView key={active.id} diff={active.text} />
          </div>
        )}
      </div>
    </div>
  );
}
