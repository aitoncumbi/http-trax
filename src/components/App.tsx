"use client";

import { useEffect, useRef, useState } from "react";

import { isTauri } from "@/lib/api";
import { useStore, useWorkspaceTabs } from "@/lib/store";

import { ActivityBar } from "./ActivityBar";
import { MainArea } from "./main/MainArea";
import { CollectionsPanel } from "./sidebar/CollectionsPanel";
import { EnvironmentsPanel } from "./sidebar/EnvironmentsPanel";
import { GitPanel } from "./sidebar/GitPanel";
import { McpPanel } from "./sidebar/McpPanel";
import { SettingsPanel } from "./sidebar/SettingsPanel";
import { TopBar } from "./TopBar";
import { Dialogs } from "./ui/Dialogs";
import { Toasts } from "./ui/Toasts";
import { NoWorkspace } from "./main/Welcome";

const SIDEBAR_KEY = "http-trax.sidebar";

function useSidebarWidth(): [number, (e: React.MouseEvent) => void] {
  const [width, setWidth] = useState(() => {
    try {
      return Number(localStorage.getItem(SIDEBAR_KEY)) || 300;
    } catch {
      return 300;
    }
  });
  const start = useRef<{ x: number; w: number } | null>(null);
  const onMouseDown = (e: React.MouseEvent) => {
    start.current = { x: e.clientX, w: width };
    const move = (ev: MouseEvent) => {
      if (!start.current) return;
      const next = Math.min(560, Math.max(220, start.current.w + ev.clientX - start.current.x));
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
      if (!mod || !active) return;
      const { saveTab, sendTab, closeTab } = useStore.getState();
      if (e.key === "s") {
        e.preventDefault();
        void saveTab(active.id);
      } else if (e.key === "Enter" && active.kind === "request") {
        e.preventDefault();
        void sendTab(active.id);
      } else if (e.key === "w") {
        e.preventDefault();
        void closeTab(active.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);
  return null;
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
    <div className="flex h-screen flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <ActivityBar />
        {ready && !hasWorkspace ? (
          <NoWorkspace />
        ) : (
          <>
            <aside className="flex min-h-0 shrink-0 flex-col border-r border-border bg-panel" style={{ width }}>
              {view === "collections" && <CollectionsPanel key={workspaceId} />}
              {view === "environments" && <EnvironmentsPanel />}
              {view === "git" && <GitPanel />}
              {view === "mcp" && <McpPanel />}
              {view === "settings" && <SettingsPanel />}
            </aside>
            <div
              role="separator"
              aria-orientation="vertical"
              onMouseDown={onResize}
              className="-ml-[3px] w-[5px] shrink-0 cursor-col-resize hover:bg-accent/40"
            />
            <main className="flex min-w-0 flex-1 flex-col">
              <MainArea />
            </main>
          </>
        )}
      </div>
      <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-border bg-panel px-3 text-[11px] text-faint">
        <span>{isTauri() ? "Desktop" : "Preview mode — mock backend (run the desktop app for real requests)"}</span>
        <span className="flex-1" />
        <span>
          <span className="kbd">Ctrl</span>+<span className="kbd">Enter</span> send · <span className="kbd">Ctrl</span>+<span className="kbd">S</span> save
        </span>
      </footer>
      <Shortcuts />
      <Dialogs />
      <Toasts />
    </div>
  );
}
