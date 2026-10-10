"use client";

import { createContext, useContext, useMemo } from "react";
import { createPortal } from "react-dom";

import { useStore } from "@/lib/store";
import type { GroupMeta } from "@/lib/types";
import { cx } from "@/lib/util";
import { buildScope, type VarInfo, type VarScope } from "@/lib/variables";

/** The variables available where an editor sits (a request, a collection). */
export const VarScopeContext = createContext<VarScope | null>(null);

export function useVarScope(): VarScope | null {
  return useContext(VarScopeContext);
}

/** Scope for something inside `collection` (the first ancestor), with the active environment. */
export function useScopeFor(collection: GroupMeta | null | undefined): VarScope {
  const tree = useStore((s) => s.tree);
  const runtime = useStore((s) => s.runtimeVars);
  const envId = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  return useMemo(
    () =>
      buildScope({
        workspace: tree?.manifest.variables,
        collection: collection ? { name: collection.name, variables: collection.variables } : null,
        environment: tree?.environments.find((e) => e.id === envId) ?? null,
        runtime,
      }),
    [tree, collection, envId, runtime],
  );
}

const SOURCE_TONE: Record<VarInfo["source"], string> = {
  workspace: "text-muted",
  collection: "text-key",
  environment: "text-ok",
  local: "text-warn",
  runtime: "text-accent-text",
  dynamic: "text-var",
};

/** What a `{{name}}` resolves to: shown on hover. */
export function VarCard({ name, info }: { name: string; info: VarInfo | undefined }) {
  return (
    <div className="max-w-[360px] min-w-[200px] rounded-[9px] border border-border-strong bg-elev px-3 py-2.5 shadow-[var(--shadow)]">
      <div className="font-mono text-[12.5px] text-var">{`{{${name}}}`}</div>
      {info ? (
        <>
          <div className="mt-1 font-mono text-[12.5px] break-all text-text">
            {info.source === "dynamic" ? info.value : info.secret ? "••••••••" : info.value || <span className="text-faint">empty</span>}
          </div>
          <div className={cx("mt-1.5 text-[11.5px]", SOURCE_TONE[info.source])}>
            {info.label}
            {info.secret ? " · secret" : ""}
          </div>
        </>
      ) : (
        <div className="mt-1 text-[12px] text-warn">Not defined in the active environment, the collection or the workspace.</div>
      )}
    </div>
  );
}

/** Suggestions under a field while typing `{{`. */
export function VarSuggestions({
  id,
  items,
  active,
  rect,
  onPick,
  onHover,
}: {
  id: string;
  items: VarInfo[];
  active: number;
  rect: DOMRect;
  onPick: (v: VarInfo) => void;
  onHover: (i: number) => void;
}) {
  const below = rect.bottom + 4;
  const top = below + 280 > window.innerHeight ? Math.max(8, rect.top - 4 - Math.min(280, items.length * 34 + 12)) : below;
  return createPortal(
    <div
      id={id}
      role="listbox"
      aria-label="Variables"
      className="fixed z-[60] w-[320px] overflow-hidden rounded-[10px] border border-border-strong bg-elev p-1 shadow-[var(--shadow)]"
      style={{ left: Math.min(rect.left, window.innerWidth - 330), top }}
    >
      {items.map((v, i) => (
        <div
          key={v.name}
          role="option"
          aria-selected={i === active}
          onMouseDown={(e) => {
            e.preventDefault();
            onPick(v);
          }}
          onMouseMove={() => onHover(i)}
          className={cx("flex h-[34px] cursor-pointer items-center gap-2.5 rounded-[7px] px-2.5", i === active && "bg-selected")}
        >
          <span className="shrink-0 font-mono text-[12.5px] text-var">{v.name}</span>
          <span className="min-w-0 flex-1 truncate text-right font-mono text-[11.5px] text-faint">
            {v.source === "dynamic" ? v.value : v.secret ? "••••••" : v.value}
          </span>
        </div>
      ))}
    </div>,
    document.body,
  );
}

/** A floating card anchored under a rectangle. */
export function FloatingCard({ rect, children }: { rect: DOMRect; children: React.ReactNode }) {
  return createPortal(
    <div className="pointer-events-none fixed z-[60]" style={{ left: Math.min(rect.left, window.innerWidth - 370), top: rect.bottom + 6 }}>
      {children}
    </div>,
    document.body,
  );
}
