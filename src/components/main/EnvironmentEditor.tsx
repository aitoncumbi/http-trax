"use client";

import { Check, RotateCcw, Save } from "lucide-react";

import { api, errorMessage } from "@/lib/api";
import { tabDirty, useStore, type EnvironmentTab } from "@/lib/store";
import { cx } from "@/lib/util";

import { KeyValueEditor, makeVar } from "../editors/KeyValueEditor";

export function EnvironmentEditor({ tab }: { tab: EnvironmentTab }) {
  const view = useStore((s) => s.tree?.environments.find((e) => e.id === tab.envId));
  const active = useStore((s) => s.workspaces.find((w) => w.id === tab.workspaceId)?.prefs.activeEnvironment === tab.envId);
  const { updateTab, saveTab, setActiveEnvironment, reloadTree, toast } = useStore.getState();
  const env = tab.draft;
  const dirty = tabDirty(tab);
  const overrides = view?.overrides ?? {};
  const extraOverrides = Object.entries(overrides).filter(([k]) => !env.variables.some((v) => v.key === k));

  const resetOverride = async (key: string) => {
    try {
      await api.setEnvironmentOverride(tab.workspaceId, tab.envId, key, null);
      await reloadTree();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3 px-4 pt-4 pb-3">
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-semibold tracking-wider text-faint uppercase">Environment</div>
          <input
            className="w-full rounded bg-transparent px-1 py-0.5 text-[17px] font-semibold outline-none hover:bg-hover focus:bg-panel-2"
            value={env.name}
            onChange={(e) => updateTab<EnvironmentTab>(tab.id, (t) => ({ ...t, draft: { ...t.draft, name: e.target.value } }))}
            aria-label="Environment name"
          />
        </div>
        <button className={cx("btn", active && "border-accent text-accent")} onClick={() => void setActiveEnvironment(active ? null : tab.envId)}>
          {active && <Check size={14} />} {active ? "Active" : "Use this environment"}
        </button>
        <button className={cx("btn", dirty && "border-accent text-accent")} disabled={!dirty} onClick={() => void saveTab(tab.id)}>
          <Save size={14} /> Save
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-4 pb-6">
        <KeyValueEditor
          rows={env.variables}
          secrets
          keyPlaceholder="Variable"
          valuePlaceholder="Shared value"
          make={makeVar}
          onChange={(variables) => updateTab<EnvironmentTab>(tab.id, (t) => ({ ...t, draft: { ...t.draft, variables } }))}
          annotate={(row) =>
            overrides[row.key] !== undefined ? (
              <span className="flex shrink-0 items-center gap-1 pr-1">
                <span className="max-w-[180px] truncate rounded bg-info/15 px-1.5 font-mono text-[10.5px] text-info" title={`Local value: ${overrides[row.key]}`}>
                  local: {overrides[row.key]}
                </span>
                <button className="icon-btn h-5 w-5" title="Reset to shared value" onClick={() => void resetOverride(row.key)}>
                  <RotateCcw size={11} />
                </button>
              </span>
            ) : null
          }
        />
        <div className="mt-3 grid gap-2 text-[11.5px] leading-relaxed text-faint md:grid-cols-2">
          <p>
            <strong className="text-muted">Shared values</strong> are committed to git with the workspace. Mark credentials with the lock to keep
            their values on this machine only.
          </p>
          <p>
            <strong className="text-muted">Local values</strong> (blue) come from <code className="font-mono">trax.env.set()</code> in scripts. They
            override the shared value here without creating git changes.
          </p>
        </div>
        {extraOverrides.length > 0 && (
          <div className="mt-5">
            <div className="label">Local-only values set by scripts</div>
            <div className="overflow-hidden rounded-md border border-border">
              {extraOverrides.map(([k, v]) => (
                <div key={k} className="flex items-center border-b border-border font-mono text-[12px] last:border-0">
                  <span className="w-1/3 truncate bg-panel-2 px-2 py-1.5 text-muted">{k}</span>
                  <span className="flex-1 truncate px-2 py-1.5" title={v}>
                    {v}
                  </span>
                  <button className="icon-btn mr-1 h-6 w-6" title="Remove local value" onClick={() => void resetOverride(k)}>
                    <RotateCcw size={12} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
