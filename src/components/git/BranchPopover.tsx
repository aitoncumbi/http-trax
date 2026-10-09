"use client";

import { ArrowLeft, ExternalLink, GitMerge, GitPullRequest, Plus, Search, Trash2 } from "lucide-react";
import { useEffect, useEffectEvent, useRef, useState, type ReactNode } from "react";

import type { Branch } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

export interface BranchActions {
  checkout: (name: string) => void;
  create: (name: string) => void;
  merge: (name: string) => void;
  remove: (name: string) => void;
  compare: (base: string) => void;
  openPullRequest: (base: string) => void;
}

function Row({
  children,
  hint,
  icon,
  active,
  current,
  onClick,
  trailing,
}: {
  children: ReactNode;
  hint?: ReactNode;
  icon?: ReactNode;
  active?: boolean;
  current?: boolean;
  onClick: () => void;
  trailing?: ReactNode;
}) {
  return (
    <div
      role="menuitem"
      tabIndex={-1}
      onClick={onClick}
      className={cx(
        "group relative flex h-8 cursor-pointer items-center gap-2 rounded-[7px] px-2.5 text-[13px]",
        current ? "bg-selected text-text" : active ? "bg-hover text-text" : "text-text-2 hover:bg-hover",
      )}
    >
      {current && <span className="absolute inset-y-0 left-0 w-[3px] rounded-sm bg-accent-line" />}
      {icon && <span className="flex w-4 justify-center text-muted">{icon}</span>}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {hint && <span className="shrink-0 text-[11.5px] text-faint">{hint}</span>}
      {trailing}
    </div>
  );
}

function Label({ children }: { children: ReactNode }) {
  return <div className="px-2.5 pt-2 pb-1 text-[11px] font-semibold tracking-[0.06em] text-faint">{children}</div>;
}

/** Branch switcher: find, switch, create, merge, compare. */
export function BranchPopover({
  branches,
  current,
  base,
  actions,
  onClose,
}: {
  branches: Branch[];
  current: string;
  /** Default branch to compare against (origin/main or main). */
  base: string;
  actions: BranchActions;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<"switch" | "merge">("switch");
  const ref = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  const close = useEffectEvent(() => onClose());
  useEffect(() => {
    input.current?.focus();
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const q = query.trim().toLowerCase();
  const local = branches.filter((b) => !b.remote);
  const localNames = new Set(local.map((b) => b.name));
  // Remote branches that have no local branch yet; checking one out creates it.
  const remote = branches.filter((b) => b.remote && !b.name.endsWith("/HEAD") && !localNames.has(b.name.replace(/^[^/]+\//, "")));
  const match = (b: Branch) => !q || b.name.toLowerCase().includes(q);
  const sortLocal = [...local].sort((a, b) => Number(b.current) - Number(a.current) || (b.lastCommitDate ?? 0) - (a.lastCommitDate ?? 0));
  const shownLocal = sortLocal.filter(match);
  const shownRemote = remote.filter(match).slice(0, 12);
  const exact = branches.some((b) => b.name.toLowerCase() === q || b.name.toLowerCase() === `origin/${q}`);
  const validName = /^[^\s~^:?*[\\]+$/.test(query.trim()) && !query.trim().endsWith("/") && !query.includes("..");

  const run = (fn: () => void) => {
    fn();
    onClose();
  };

  const submit = () => {
    if (mode === "merge") {
      const first = [...shownLocal.filter((b) => !b.current), ...shownRemote][0];
      if (first) run(() => actions.merge(first.name));
      return;
    }
    const hit = shownLocal.find((b) => b.name.toLowerCase() === q) ?? (q ? shownLocal[0] : undefined) ?? shownRemote[0];
    if (hit && (exact || !validName)) {
      if (!hit.current) run(() => actions.checkout(hit.name));
    } else if (q && validName) run(() => actions.create(query.trim()));
  };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={mode === "merge" ? `Merge into ${current}` : "Branches"}
      className="absolute top-11 left-3 z-20 w-[300px] rounded-[10px] border border-border-strong bg-elev p-1.5 shadow-[var(--shadow)]"
    >
      <div className="mb-1 flex h-[30px] items-center gap-2 rounded-[7px] border border-border-strong px-2.5 focus-within:border-accent-line">
        {mode === "merge" ? (
          <button type="button" aria-label="Back to branches" className="text-muted hover:text-text" onClick={() => setMode("switch")}>
            <ArrowLeft size={13} />
          </button>
        ) : (
          <Search size={13} className="text-faint" />
        )}
        <input
          ref={input}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder={mode === "merge" ? `Merge into ${current}…` : "Find or create a branch…"}
          aria-label={mode === "merge" ? "Find a branch to merge" : "Find or create a branch"}
          className="h-full min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-faint"
        />
      </div>

      <div className="max-h-[340px] overflow-y-auto">
        {mode === "switch" && q && validName && !exact && (
          <Row icon={<Plus size={13} />} active onClick={() => run(() => actions.create(query.trim()))} hint={`from ${current}`}>
            Create <span className="font-medium text-text">{query.trim()}</span>
          </Row>
        )}
        {shownLocal
          .filter((b) => mode === "switch" || !b.current)
          .map((b) => (
            <Row
              key={b.name}
              current={mode === "switch" && b.current}
              onClick={() => run(() => (mode === "merge" ? actions.merge(b.name) : !b.current && actions.checkout(b.name)))}
              hint={b.current ? "current" : b.ahead || b.behind ? `↑${b.ahead} ↓${b.behind}` : b.lastCommitDate ? timeAgo(b.lastCommitDate) : undefined}
              trailing={
                mode === "switch" && !b.current ? (
                  <button
                    type="button"
                    aria-label={`Delete ${b.name}`}
                    title="Delete branch"
                    onClick={(e) => {
                      e.stopPropagation();
                      run(() => actions.remove(b.name));
                    }}
                    className="hidden h-6 w-6 shrink-0 items-center justify-center rounded text-faint group-hover:flex hover:bg-selected hover:text-danger"
                  >
                    <Trash2 size={12} />
                  </button>
                ) : undefined
              }
            >
              {b.name}
            </Row>
          ))}
        {shownRemote.length > 0 && (
          <>
            <Label>REMOTE</Label>
            {shownRemote.map((b) => (
              <Row
                key={b.name}
                onClick={() => run(() => (mode === "merge" ? actions.merge(b.name) : actions.checkout(b.name)))}
                hint={b.lastCommitDate ? timeAgo(b.lastCommitDate) : undefined}
              >
                {b.name}
              </Row>
            ))}
          </>
        )}
        {shownLocal.length === 0 && shownRemote.length === 0 && !(q && validName) && (
          <div className="px-2.5 py-2 text-[12.5px] text-faint">No branches match.</div>
        )}
      </div>

      {mode === "switch" && (
        <>
          <div className="mx-1 my-1.5 h-px bg-border" />
          <Row icon={<Plus size={13} />} onClick={() => (q && validName ? run(() => actions.create(query.trim())) : input.current?.focus())}>
            New branch from {current}
          </Row>
          <Row
            icon={<GitMerge size={13} />}
            onClick={() => {
              setMode("merge");
              setQuery("");
              input.current?.focus();
            }}
          >
            Merge a branch into {current}…
          </Row>
          {base !== current && base.replace(/^origin\//, "") !== current && (
            <>
              <Row icon={<GitPullRequest size={13} />} onClick={() => run(() => actions.compare(base))}>
                Review changes against {base}
              </Row>
              <Row icon={<ExternalLink size={13} />} onClick={() => run(() => actions.openPullRequest(base))}>
                Open a pull request
              </Row>
            </>
          )}
        </>
      )}
    </div>
  );
}
