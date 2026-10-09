"use client";

import { Check, RotateCcw } from "lucide-react";

import { api, errorMessage } from "@/lib/api";
import { tabDirty, useStore, type EnvironmentTab } from "@/lib/store";
import { cx, envTone } from "@/lib/util";

import { KeyValueEditor, makeVar } from "../editors/KeyValueEditor";
import { PageHeader, SaveButton } from "./PageHeader";

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
      <PageHeader
        eyebrow={
          <span className="inline-flex items-center gap-2">
            <span className="h-[7px] w-[7px] rounded-full" style={{ background: envTone(env.name) }} aria-hidden />
            ENVIRONMENT
          </span>
        }
        title={env.name}
        onTitle={(name) => updateTab<EnvironmentTab>(tab.id, (t) => ({ ...t, draft: { ...t.draft, name } }))}
      >
        <button
          type="button"
          className={cx("btn h-9 px-3.5", active && "border-accent-line text-accent-text")}
          onClick={() => void setActiveEnvironment(active ? null : tab.envId)}
          title={active ? "Stop using this environment" : undefined}
        >
          {active && <Check size={14} strokeWidth={1.8} />} {active ? "Active" : "Use this environment"}
        </button>
        <SaveButton dirty={dirty} onSave={() => void saveTab(tab.id)} />
      </PageHeader>
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
                <span className="max-w-[180px] truncate rounded bg-info/15 px-1.5 py-px font-mono text-[11px] text-info" title={`Local value: ${overrides[row.key]}`}>
                  local: {overrides[row.key]}
                </span>
                <button className="icon-btn h-5 w-5" title="Reset to shared value" onClick={() => void resetOverride(row.key)}>
                  <RotateCcw size={11} />
                </button>
              </span>
            ) : null
          }
        />
        <div className="mx-1 mt-4 grid gap-4 text-[12.5px] leading-[19px] text-faint md:grid-cols-2">
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
            <div className="overflow-hidden rounded-lg border border-border">
              {extraOverrides.map(([k, v]) => (
                <div key={k} className="flex h-[38px] items-center border-b border-border-soft font-mono text-[13px] last:border-0">
                  <span className="w-1/3 truncate px-3 text-key">{k}</span>
                  <span className="flex-1 truncate px-3 text-muted" title={v}>
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
