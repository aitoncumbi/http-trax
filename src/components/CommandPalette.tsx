"use client";

import {
  Bot,
  Code2,
  FolderTree,
  GitBranch,
  LayoutGrid,
  Moon,
  PanelsTopLeft,
  Play,
  Plus,
  RefreshCw,
  Search,
  SlidersHorizontal,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { GroupNode, HistoryEntry, TreeItem } from "@/lib/types";
import { cx, envTone } from "@/lib/util";

import { MethodPill, WorkspaceAvatar } from "./ui/Brand";

interface Item {
  id: string;
  label: string;
  detail?: string;
  method?: string;
  icon?: React.ReactNode;
  shortcut?: string;
  section: string;
  run: () => void;
}

interface FlatRequest {
  id: string;
  name: string;
  method: string;
  url: string;
  path: string;
}

function flatten(collections: GroupNode[]): FlatRequest[] {
  const out: FlatRequest[] = [];
  const walk = (items: TreeItem[], trail: string[]) => {
    for (const item of items) {
      if (item.kind === "request") out.push({ id: item.id, name: item.name, method: item.method, url: item.url, path: trail.join(" / ") });
      else walk(item.items, [...trail, item.meta.name]);
    }
  };
  for (const c of collections) walk(c.items, [c.meta.name]);
  return out;
}

/** Subsequence match with a score that prefers contiguous, early matches. */
function score(text: string, query: string): number {
  const t = text.toLowerCase();
  const q = query.toLowerCase();
  const direct = t.indexOf(q);
  if (direct >= 0) return 1000 - direct;
  let ti = 0;
  let gaps = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return -1;
    gaps += found - ti;
    ti = found + 1;
  }
  return 500 - gaps;
}

export function CommandPalette() {
  const mode = useStore((s) => s.palette);
  const tree = useStore((s) => s.tree);
  const workspaces = useStore((s) => s.workspaces);
  const activeId = useStore((s) => s.activeWorkspaceId);
  const activeEnv = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const s = useStore.getState();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!mode) return;
    inputRef.current?.focus();
    if (activeId) api.getHistory(activeId).then(setHistory).catch(() => setHistory([]));
  }, [mode, activeId]);

  const close = () => {
    s.closePalette();
    setQuery("");
    setIndex(0);
  };

  const requests = useMemo(() => flatten(tree?.collections ?? []), [tree]);

  const items: Item[] = useMemo(() => {
    if (!mode) return [];
    const q = query.trim();
    if (mode === "workspaces") {
      return workspaces
        .map((w, i) => ({
          id: `ws:${w.id}`,
          label: w.name,
          detail: w.team ?? undefined,
          icon: <WorkspaceAvatar id={w.id} name={w.name} />,
          shortcut: i < 9 ? `Ctrl ${i + 1}` : undefined,
          section: "Workspaces",
          run: () => void s.selectWorkspace(w.id),
          s: q ? score(w.name, q) : 1,
        }))
        .filter((x) => x.s >= 0)
        .sort((a, b) => b.s - a.s);
    }
    if (mode === "environments") {
      const envs = [{ id: null as string | null, name: "No environment" }, ...(tree?.environments ?? []).map((e) => ({ id: e.id as string | null, name: e.name }))];
      return envs
        .map((e) => ({
          id: `env:${e.id}`,
          label: e.name,
          detail: e.id === activeEnv ? "Active" : undefined,
          icon: <span className="mx-auto block h-2 w-2 rounded-full" style={{ background: envTone(e.id ? e.name : null) }} />,
          section: "Environments",
          run: () => void s.setActiveEnvironment(e.id),
          s: q ? score(e.name, q) : 1,
        }))
        .filter((x) => x.s >= 0);
    }

    const open = (id: string) => () => {
      s.setView("collections");
      void s.openRequest(id);
    };
    const commands: Item[] = [
      { id: "new", label: "New request", icon: <Plus size={17} strokeWidth={1.5} />, shortcut: "Ctrl N", section: "Commands", run: () => void s.newRequestHere() },
      { id: "ws", label: "Switch workspace", icon: <LayoutGrid size={16} strokeWidth={1.4} />, shortcut: "Ctrl 1–9", section: "Commands", run: () => s.openPalette("workspaces") },
      { id: "env", label: "Switch environment", icon: <SlidersHorizontal size={16} strokeWidth={1.4} />, shortcut: "Ctrl E", section: "Commands", run: () => s.openPalette("environments") },
      { id: "curl", label: "Import from cURL", icon: <Code2 size={16} strokeWidth={1.4} />, shortcut: "Ctrl Shift I", section: "Commands", run: () => s.showImportDialog() },
      { id: "collection", label: "New collection", icon: <FolderTree size={16} strokeWidth={1.4} />, section: "Commands", run: () => void s.createCollection() },
      { id: "git", label: "Open source control", icon: <GitBranch size={16} strokeWidth={1.4} />, section: "Commands", run: () => s.setView("git") },
      { id: "sync", label: "Sync with remote now", icon: <RefreshCw size={16} strokeWidth={1.4} />, section: "Commands", run: () => void s.syncNow() },
      {
        id: "layout",
        label: s.layout === "stack" ? "Show response side by side" : "Stack response under request",
        icon: <PanelsTopLeft size={16} strokeWidth={1.4} />,
        section: "Commands",
        run: () => s.setLayout(s.layout === "stack" ? "side" : "stack"),
      },
      { id: "theme", label: s.theme === "dark" ? "Use light theme" : "Use dark theme", icon: <Moon size={16} strokeWidth={1.4} />, section: "Commands", run: () => s.toggleTheme() },
      { id: "mcp", label: "Connect an AI agent (MCP)", icon: <Bot size={16} strokeWidth={1.4} />, section: "Commands", run: () => s.setView("mcp") },
      ...(tree?.collections ?? []).map((c) => ({
        id: `run:${c.meta.id}`,
        label: `Run ${c.meta.name}`,
        icon: <Play size={15} strokeWidth={1.5} />,
        section: "Commands",
        run: () => void s.runGroup(c.meta.id),
      })),
    ];

    if (!q) {
      const seen = new Set<string>();
      const recent: Item[] = [];
      for (const h of history) {
        if (!h.requestId || seen.has(h.requestId)) continue;
        const req = requests.find((r) => r.id === h.requestId);
        if (!req) continue;
        seen.add(h.requestId);
        recent.push({ id: `r:${req.id}`, label: req.name, detail: req.path, method: req.method, section: "Recent", run: open(req.id) });
        if (recent.length >= 4) break;
      }
      if (recent.length === 0) {
        for (const req of requests.slice(0, 4)) {
          recent.push({ id: `r:${req.id}`, label: req.name, detail: req.path, method: req.method, section: "Requests", run: open(req.id) });
        }
      }
      return [...recent, ...commands.filter((c) => !c.id.startsWith("run:"))];
    }

    const matchedRequests = requests
      .map((r) => ({ r, s: Math.max(score(r.name, q), score(`${r.method} ${r.url}`, q) - 100, score(r.path, q) - 200) }))
      .filter((x) => x.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 8)
      .map(({ r }) => ({ id: `r:${r.id}`, label: r.name, detail: r.path, method: r.method, section: "Requests", run: open(r.id) }));
    const matchedCommands = commands
      .map((c) => ({ c, s: score(c.label, q) }))
      .filter((x) => x.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 6)
      .map((x) => x.c);
    return [...matchedRequests, ...matchedCommands];
  }, [mode, query, history, requests, workspaces, tree, activeEnv, s]);

  // Keep the selection in range and visible.
  const current = Math.min(index, Math.max(0, items.length - 1));
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${current}"]`)?.scrollIntoView({ block: "nearest" });
  }, [current]);

  if (!mode) return null;

  const choose = (item: Item | undefined) => {
    if (!item) return;
    const keepOpen = item.id === "ws" || item.id === "env";
    if (keepOpen) {
      setQuery("");
      setIndex(0);
      item.run();
      return;
    }
    close();
    item.run();
  };

  const placeholder =
    mode === "workspaces" ? "Switch to workspace…" : mode === "environments" ? "Switch environment…" : "Search requests or run a command";

  return (
    <div
      className="fixed inset-0 z-50 bg-[var(--scrim)] backdrop-blur-[4px]"
      onMouseDown={(e) => e.target === e.currentTarget && close()}
    >
      <div
        role="dialog"
        aria-label="Command palette"
        className="mx-auto mt-[120px] w-[660px] max-w-[calc(100vw-32px)] overflow-hidden rounded-[14px] border border-border-strong bg-elev shadow-[var(--shadow)]"
      >
        <div className="flex h-[58px] items-center gap-3 border-b border-border pr-[18px] pl-5">
          <Search size={18} strokeWidth={1.4} className="text-muted" />
          <label htmlFor="palette-q" className="sr-only">
            {placeholder}
          </label>
          <input
            id="palette-q"
            ref={inputRef}
            value={query}
            placeholder={placeholder}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => {
              setQuery(e.target.value);
              setIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setIndex(Math.min(items.length - 1, current + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setIndex(Math.max(0, current - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                choose(items[current]);
              } else if (e.key === "Escape") {
                e.preventDefault();
                if (mode !== "search") {
                  s.openPalette("search");
                  setQuery("");
                } else close();
              } else if (e.key === "Backspace" && !query && mode !== "search") {
                s.openPalette("search");
              }
            }}
            className="min-w-0 flex-1 bg-transparent text-[16px] text-text outline-none placeholder:text-faint"
          />
          <kbd className="kbd">Esc</kbd>
        </div>
        <div ref={listRef} className="max-h-[440px] overflow-y-auto p-2">
          {items.length === 0 && <p className="px-3 py-6 text-center text-[13.5px] text-faint">Nothing matches “{query}”.</p>}
          {items.map((item, i) => {
            const header = i === 0 || item.section !== items[i - 1].section ? item.section : null;
            const selected = i === current;
            return (
              <div key={item.id}>
                {header && (
                  <div className={cx("px-3 pb-1.5 text-[11.5px] font-semibold tracking-[0.04em] text-faint", i === 0 ? "pt-2" : "pt-3.5")}>{header}</div>
                )}
                <button
                  type="button"
                  data-index={i}
                  onMouseMove={() => setIndex(i)}
                  onClick={() => choose(item)}
                  className={cx("relative flex h-11 w-full items-center gap-3 rounded-[9px] px-3 text-left", selected && "bg-selected")}
                >
                  {selected && <span className="absolute top-0 bottom-0 left-0 w-[3px] rounded-sm bg-accent-line" />}
                  <span className="flex w-11 shrink-0 justify-center text-muted">
                    {item.method ? <MethodPill method={item.method} className="w-11 py-[3px] text-[10.5px]" /> : item.icon}
                  </span>
                  <span className={cx("truncate text-[14.5px]", selected ? "font-medium text-text" : "text-text-2")}>{item.label}</span>
                  <span className="flex-1 truncate text-[12.5px] text-faint">{item.detail}</span>
                  {selected ? (
                    <kbd className="rounded-[5px] bg-accent px-[7px] py-[2px] text-[10.5px] font-semibold text-white">↵ Open</kbd>
                  ) : (
                    item.shortcut && <kbd className="kbd">{item.shortcut}</kbd>
                  )}
                </button>
              </div>
            );
          })}
        </div>
        <div className="flex h-10 items-center justify-end gap-[18px] border-t border-border bg-bg px-[18px] text-[12px] text-muted">
          <span className="flex items-center gap-1.5">
            Navigate <kbd className="kbd">↑ ↓</kbd>
          </span>
          <span className="flex items-center gap-1.5">
            Open <kbd className="kbd">↵</kbd>
          </span>
          {mode !== "search" && (
            <span className="flex items-center gap-1.5">
              Back <kbd className="kbd">Esc</kbd>
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
