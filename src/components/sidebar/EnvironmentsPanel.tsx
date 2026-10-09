"use client";

import { Eraser, MoreHorizontal, Pencil, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { LocalState } from "@/lib/types";
import { cx, envTone } from "@/lib/util";

import { MenuButton } from "../ui/Menu";

export function EnvironmentsPanel() {
  const tree = useStore((s) => s.tree);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const activeEnv = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const { setActiveEnvironment, openEnvironment, createEnvironment, confirm, reloadTree, toast } = useStore.getState();
  const [local, setLocal] = useState<LocalState | null>(null);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .getLocalState(workspaceId)
      .then((l) => !cancelled && setLocal(l))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [workspaceId, tree]);

  const envs = tree?.environments ?? [];
  const current = envs.find((e) => e.id === activeEnv) ?? null;
  const runtime = Object.entries(local?.runtimeVariables ?? {});

  const rows: { key: string; value: string; tag: string }[] = current
    ? [
        ...current.variables.map((v) => ({
          key: v.key,
          value: v.secret ? "••••••••" : (current.overrides[v.key] ?? v.value) || "not set",
          tag: v.secret ? "secret" : current.overrides[v.key] !== undefined ? "local" : v.enabled ? "" : "off",
        })),
        ...Object.entries(current.overrides)
          .filter(([k]) => !current.variables.some((v) => v.key === k))
          .map(([key, value]) => ({ key, value, tag: "script" })),
      ]
    : [];

  const remove = async (id: string, name: string) => {
    if (!workspaceId) return;
    const ok = await confirm({ title: `Delete ${name}?`, message: "The environment file is removed from the workspace.", confirm: "Delete", danger: true });
    if (!ok) return;
    try {
      await api.deleteEnvironment(workspaceId, id);
      if (id === activeEnv) await setActiveEnvironment(null);
      await reloadTree();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="flex shrink-0 items-center justify-between pt-[18px] pr-3.5 pb-3 pl-[18px]">
        <h2 className="text-[15px] font-semibold">Environments</h2>
        <button type="button" className="icon-btn" aria-label="New environment" title="New environment" onClick={() => void createEnvironment()}>
          <Plus size={16} strokeWidth={1.6} />
        </button>
      </div>

      <div className="flex flex-col gap-0.5 px-2.5">
        <button
          type="button"
          aria-pressed={!activeEnv}
          onClick={() => void setActiveEnvironment(null)}
          className={cx("flex h-10 items-center gap-3 rounded-lg px-3 text-left text-[14.5px]", !activeEnv ? "bg-selected font-semibold" : "hover:bg-hover")}
        >
          <span className="h-2 w-2 rounded-full" style={{ background: envTone(null) }} aria-hidden />
          <span className="flex-1">No Environment</span>
          {!activeEnv && <span className="text-[12px] font-normal text-faint">Active</span>}
        </button>
        {envs.map((env) => {
          const on = env.id === activeEnv;
          const localCount = Object.keys(env.overrides).length;
          return (
            <div key={env.id} className={cx("group flex h-10 items-center rounded-lg pr-1", on ? "bg-selected" : "hover:bg-hover")}>
              <button
                type="button"
                aria-pressed={on}
                onClick={() => void setActiveEnvironment(env.id)}
                onDoubleClick={() => openEnvironment(env.id)}
                className="flex h-full min-w-0 flex-1 items-center gap-3 pl-3 text-left text-[14.5px]"
              >
                <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: envTone(env.name) }} aria-hidden />
                <span className={cx("flex-1 truncate", on && "font-semibold")}>{env.name}</span>
                <span className="text-[12px] text-faint group-hover:hidden">{on ? "Active" : localCount ? `${localCount} local` : ""}</span>
              </button>
              <MenuButton
                title="Environment actions"
                items={[
                  { label: "Edit variables", icon: <Pencil size={14} />, onSelect: () => openEnvironment(env.id) },
                  { label: "", separator: true },
                  { label: "Delete", icon: <Trash2 size={14} />, danger: true, onSelect: () => void remove(env.id, env.name) },
                ]}
                className="hidden h-7 w-7 items-center justify-center rounded-md text-faint group-hover:flex hover:bg-hover"
              >
                <MoreHorizontal size={15} />
              </MenuButton>
            </div>
          );
        })}
      </div>

      {current && (
        <>
          <div className="flex items-center justify-between px-[18px] pt-[22px] pb-2">
            <span className="eyebrow">Variables · {current.name}</span>
            <button type="button" className="text-[12.5px] text-accent-text hover:underline" onClick={() => openEnvironment(current.id)}>
              Edit
            </button>
          </div>
          <div className="px-2.5 pb-3">
            {rows.length === 0 && <p className="border-t border-border-soft px-2 py-2.5 text-[12.5px] text-faint">No variables</p>}
            {rows.map((r) => (
              <div key={r.key} className="flex min-h-[38px] items-center gap-2.5 border-t border-border-soft px-2 py-1.5">
                <span className="w-[38%] shrink-0 truncate font-mono text-[12.5px] text-var">{r.key}</span>
                <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-muted" title={r.value}>
                  {r.value}
                </span>
                {r.tag && <span className="text-[11px] text-faint">{r.tag}</span>}
              </div>
            ))}
          </div>
        </>
      )}

      {envs.length === 0 && (
        <p className="mx-[18px] mt-4 text-[12.5px] leading-[19px] text-faint">
          Environments hold values like <span className="font-mono text-var">baseUrl</span> for each place you send requests: local, staging,
          production.
        </p>
      )}

      <div className="mt-auto px-[18px] pt-5 pb-4">
        <div className="mb-2 flex items-center justify-between">
          <span className="eyebrow">Runtime variables</span>
          {runtime.length > 0 && workspaceId && (
            <button
              type="button"
              className="btn-ghost h-6 text-[12px]"
              onClick={async () => {
                await api.clearRuntimeVariables(workspaceId);
                setLocal(await api.getLocalState(workspaceId));
              }}
            >
              <Eraser size={12} /> Clear
            </button>
          )}
        </div>
        {runtime.length === 0 ? (
          <p className="text-[12.5px] leading-[18px] text-faint">
            None. Scripts set these with <span className="font-mono">trax.vars.set()</span>; they stay on this machine.
          </p>
        ) : (
          runtime.map(([k, v]) => (
            <div key={k} className="flex items-center gap-2.5 border-t border-border-soft py-1.5">
              <span className="w-[38%] shrink-0 truncate font-mono text-[12.5px] text-var">{k}</span>
              <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-muted" title={v}>
                {v}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
