"use client";

import { useEffect, useRef, useState } from "react";

import { useStore, useWorkspaceTabs } from "@/lib/store";

import { ActivityBar } from "./ActivityBar";
import { CommandPalette } from "./CommandPalette";
import { GitChanges, GitMain } from "./git/GitView";
import { AgentsView } from "./main/AgentsView";
import { EnvironmentView } from "./main/EnvironmentView";
import { MainArea } from "./main/MainArea";
import { SettingsView } from "./main/SettingsView";
import { NoWorkspace } from "./main/Welcome";
import { AgentsPanel } from "./sidebar/AgentsPanel";
import { CollectionsPanel } from "./sidebar/CollectionsPanel";
import { EnvironmentsPanel } from "./sidebar/EnvironmentsPanel";
import { HistoryPanel } from "./sidebar/HistoryPanel";
import { SettingsPanel } from "./sidebar/SettingsPanel";
import { Sidebar } from "./sidebar/Sidebar";
import { TitleBar } from "./TitleBar";
import { Dialogs } from "./ui/Dialogs";
import { Toasts } from "./ui/Toasts";

const SIDEBAR_KEY = "http-trax.sidebar";

function useSidebarWidth(): [number, (e: React.MouseEvent) => void] {
  const [width, setWidth] = useState(() => {
    try {
      return Number(localStorage.getItem(SIDEBAR_KEY)) || 236;
    } catch {
      return 236;
    }
  });
  const start = useRef<{ x: number; w: number } | null>(null);
  const onMouseDown = (e: React.MouseEvent) => {
    start.current = { x: e.clientX, w: width };
    const move = (ev: MouseEvent) => {
      if (!start.current) return;
      const next = Math.min(480, Math.max(208, start.current.w + ev.clientX - start.current.x));
      setWidth(next);
    };
    const up = () => {
      start.current = null;
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      setWidth((w) => {
        try {
          localStorage.setItem(SIDEBAR_KEY, String(w));
        } catch {
          // storage unavailable
        }
        return w;
      });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  return [width, onMouseDown];
}

function Shortcuts() {
  const { active } = useWorkspaceTabs();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      const st = useStore.getState();
      const key = e.key.toLowerCase();
      if (key === "k") {
        e.preventDefault();
        if (st.palette) st.closePalette();
        else st.openPalette();
        return;
      }
      // Leave keys alone while a dialog or the palette has focus.
      if (st.palette || st.dialog) return;
      if (key === "n" && !e.shiftKey) {
        e.preventDefault();
        void st.newRequestHere();
      } else if (key === "e" && !e.shiftKey) {
        e.preventDefault();
        st.openPalette("environments");
      } else if (key === "i" && e.shiftKey) {
        e.preventDefault();
        st.showImportDialog();
      } else if (/^[1-9]$/.test(e.key) && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        void st.selectWorkspaceByIndex(Number(e.key) - 1);
      } else if (!active) {
        return;
      } else if (key === "s") {
        e.preventDefault();
        void st.saveTab(active.id);
      } else if (e.key === "Enter" && active.kind === "request") {
        e.preventDefault();
        void st.sendTab(active.id);
      } else if (key === "w") {
        e.preventDefault();
        void st.closeTab(active.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);
  return null;
}

/** The inset, rounded surface that holds the main content. */
function Card({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-w-0 flex-1 pr-2 pb-2">
      <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-bg">{children}</main>
    </div>
  );
}

export default function App() {
  const init = useStore((s) => s.init);
  const ready = useStore((s) => s.ready);
  const view = useStore((s) => s.view);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const hasWorkspace = workspaceId !== null;
  const [width, onResize] = useSidebarWidth();

  useEffect(() => {
    void init();
  }, [init]);

  // Fetch when the window regains focus (at most every 30s): feels "always online".
  const lastFocusSync = useRef(0);
  useEffect(() => {
    const onFocus = () => {
      if (Date.now() - lastFocusSync.current < 30_000) return;
      lastFocusSync.current = Date.now();
      void useStore.getState().syncNow();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  return (
    <div className="flex h-screen flex-col bg-frame">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <ActivityBar />
        {ready && !hasWorkspace ? (
          <Card>
            <NoWorkspace />
          </Card>
        ) : (
          <>
            <Sidebar width={width}>
              {view === "collections" && <CollectionsPanel key={workspaceId} />}
              {view === "environments" && <EnvironmentsPanel />}
              {view === "history" && <HistoryPanel key={workspaceId} />}
              {view === "mcp" && <AgentsPanel key={workspaceId} />}
              {view === "git" && <GitChanges />}
              {view === "settings" && <SettingsPanel />}
            </Sidebar>
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize sidebar"
              onMouseDown={onResize}
              className="-ml-1.5 w-1.5 shrink-0 cursor-col-resize"
            />
            <Card>
              {view === "git" ? (
                <GitMain key={workspaceId} />
              ) : view === "environments" ? (
                <EnvironmentView key={workspaceId} />
              ) : view === "mcp" ? (
                <AgentsView key={workspaceId} />
              ) : view === "settings" ? (
                <SettingsView />
              ) : (
                <MainArea />
              )}
            </Card>
          </>
        )}
      </div>
      <Shortcuts />
      <CommandPalette />
      <Dialogs />
      <Toasts />
    </div>
  );
}
