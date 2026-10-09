"use client";

import { Copy, Pencil, Plus, Trash2 } from "lucide-react";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { EnvironmentView } from "@/lib/types";
import { cx, envTone } from "@/lib/util";

import { useContextMenu } from "../ui/Menu";

function EnvRow({ env, active }: { env: EnvironmentView; active: boolean }) {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const { setActiveEnvironment, confirm, prompt, reloadTree, toast } = useStore.getState();

  const run = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      await reloadTree();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const ctx = useContextMenu(() => [
    {
      label: "Rename…",
      icon: <Pencil size={14} />,
      onSelect: async () => {
        const name = await prompt({ title: "Rename environment", label: "Name", initial: env.name, confirm: "Rename" });
        if (name && workspaceId) await run(() => api.saveEnvironment(workspaceId, { id: env.id, name, protected: env.protected, variables: env.variables }));
      },
    },
    {
      label: "Duplicate",
      icon: <Copy size={14} />,
      onSelect: () =>
        workspaceId &&
        void run(() => api.saveEnvironment(workspaceId, { id: "", name: `${env.name} copy`, protected: env.protected, variables: env.variables })),
    },
    { label: "", separator: true },
    {
      label: "Delete",
      icon: <Trash2 size={14} />,
      danger: true,
      onSelect: async () => {
        if (!workspaceId) return;
        const ok = await confirm({ title: `Delete ${env.name}?`, message: "The environment file is removed from the workspace.", confirm: "Delete", danger: true });
        if (!ok) return;
        await run(async () => {
          await api.deleteEnvironment(workspaceId, env.id);
          if (active) await setActiveEnvironment(null);
        });
      },
    },
  ]);

  return (
    <>
      <button
      type="button"
      aria-pressed={active}
      onClick={() => void setActiveEnvironment(env.id)}
      onContextMenu={ctx.open}
      className={cx("flex h-10 w-full items-center gap-3 rounded-lg px-3 text-left text-[14.5px]", active ? "bg-selected font-semibold" : "hover:bg-hover")}
    >
      <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: envTone(env.name) }} aria-hidden />
      <span className="min-w-0 flex-1 truncate">{env.name}</span>
      <span className="text-[12px] font-normal text-faint">{active ? "Active" : env.protected ? "Protected" : ""}</span>
      </button>
      {/* Outside the button: the menu is portalled, but React events still bubble. */}
      {ctx.menu}
    </>
  );
}

/** Pick the active environment; the main card shows and edits it. */
export function EnvironmentsPanel() {
  const tree = useStore((s) => s.tree);
  const activeEnv = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const { setActiveEnvironment, createEnvironment } = useStore.getState();
  const envs = tree?.environments ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="flex shrink-0 items-center justify-between pt-3.5 pr-1 pb-3 pl-2.5">
        <h2 className="text-[15px] font-semibold">Environments</h2>
        <button type="button" className="icon-btn" aria-label="New environment" title="New environment" onClick={() => void createEnvironment()}>
          <Plus size={16} strokeWidth={1.6} />
        </button>
      </div>
      <div className="flex flex-col gap-0.5">
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
        {envs.map((env) => (
          <EnvRow key={env.id} env={env} active={env.id === activeEnv} />
        ))}
      </div>
      {envs.length === 0 && (
        <p className="mx-3 mt-3 text-[12.5px] leading-[18px] text-faint">
          Environments hold the values that change between Local, Staging and Production, like the base URL and tokens.
        </p>
      )}
    </div>
  );
}
