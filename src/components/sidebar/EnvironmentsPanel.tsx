"use client";

import { Check, Eraser, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { LocalState } from "@/lib/types";
import { cx } from "@/lib/util";

export function EnvironmentsPanel() {
  const tree = useStore((s) => s.tree);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const activeEnv = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const { setActiveEnvironment, openEnvironment, createEnvironment, confirm, reloadTree, toast } = useStore.getState();
  const [local, setLocal] = useState<LocalState | null>(null);

  useEffect(() => {
    if (!workspaceId) return;
    api.getLocalState(workspaceId).then(setLocal).catch(() => setLocal(null));
  }, [workspaceId, tree]);

  const envs = tree?.environments ?? [];
  const runtime = Object.entries(local?.runtimeVariables ?? {});

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-3 pt-3 pb-2">
        <h2 className="text-[11px] font-semibold tracking-wider text-faint uppercase">Environments</h2>
        <button className="icon-btn" title="New environment" onClick={() => void createEnvironment()}>
          <Plus size={15} />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-4">
        {envs.length === 0 && (
          <div className="mx-1 mt-1 rounded-lg border border-dashed border-border-strong p-4 text-center text-[12.5px] text-muted">
            No environments yet. Environments hold values like <code className="font-mono">baseUrl</code> per target (local, staging…).
            <div>
              <button className="btn-primary mt-3" onClick={() => void createEnvironment()}>
                <Plus size={14} /> New environment
              </button>
            </div>
          </div>
        )}
        {envs.map((env) => {
          const active = env.id === activeEnv;
          const overrides = Object.keys(env.overrides).length;
          return (
            <div key={env.id} className={cx("group mb-1 flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-hover", active && "bg-accent-soft")}>
              <button
                title={active ? "Active environment" : "Use this environment"}
                onClick={() => void setActiveEnvironment(active ? null : env.id)}
                className={cx(
                  "flex h-4 w-4 shrink-0 items-center justify-center rounded-full border",
                  active ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
                )}
              >
                {active && <Check size={10} strokeWidth={3} />}
              </button>
              <button className="min-w-0 flex-1 text-left" onClick={() => openEnvironment(env.id)}>
                <div className="truncate font-medium">{env.name}</div>
                <div className="text-[11px] text-faint">
                  {env.variables.length} variable{env.variables.length === 1 ? "" : "s"}
                  {overrides > 0 && ` · ${overrides} local value${overrides === 1 ? "" : "s"}`}
                </div>
              </button>
              <button
                className="icon-btn opacity-0 group-hover:opacity-100"
                title="Delete environment"
                onClick={async () => {
                  if (!workspaceId) return;
                  const ok = await confirm({ title: `Delete "${env.name}"?`, message: "The environment file is removed.", confirm: "Delete", danger: true });
                  if (!ok) return;
                  try {
                    await api.deleteEnvironment(workspaceId, env.id);
                    if (active) await setActiveEnvironment(null);
                    await reloadTree();
                  } catch (e) {
                    toast("error", errorMessage(e));
                  }
                }}
              >
                <Trash2 size={13} />
              </button>
            </div>
          );
        })}

        <div className="mt-5 px-1">
          <div className="mb-1.5 flex items-center justify-between">
            <h3 className="text-[11px] font-semibold tracking-wider text-faint uppercase">Runtime variables</h3>
            {runtime.length > 0 && (
              <button
                className="btn-ghost h-6"
                onClick={async () => {
                  if (!workspaceId) return;
                  await api.clearRuntimeVariables(workspaceId);
                  setLocal(await api.getLocalState(workspaceId));
                }}
              >
                <Eraser size={12} /> Clear
              </button>
            )}
          </div>
          <p className="mb-2 text-[11.5px] leading-relaxed text-faint">
            Set by scripts with <code className="font-mono">trax.vars.set()</code>. They override every environment and stay on this machine.
          </p>
          {runtime.length === 0 ? (
            <p className="text-[12px] text-faint italic">None</p>
          ) : (
            <div className="overflow-hidden rounded-md border border-border">
              {runtime.map(([k, v]) => (
                <div key={k} className="flex border-b border-border font-mono text-[11.5px] last:border-0">
                  <span className="w-2/5 truncate bg-panel-2 px-2 py-1 text-muted">{k}</span>
                  <span className="flex-1 truncate px-2 py-1" title={v}>
                    {v}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
