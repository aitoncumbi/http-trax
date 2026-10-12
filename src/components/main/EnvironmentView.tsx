"use client";

import { Eye, EyeOff, Lock, MoreHorizontal, Plus, RotateCcw } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { Environment, EnvironmentView as EnvView, LocalState, Variable } from "@/lib/types";
import { cx, envTone, stableStringify } from "@/lib/util";

import { Switch } from "../ui/Controls";
import { MenuButton } from "../ui/Menu";

const fromView = (e: EnvView): Environment => ({
  id: e.id,
  name: e.name,
  protected: e.protected,
  variables: e.variables.map((v) => ({ ...v })),
});

function SettingCard({ icon, title, desc, children }: { icon: ReactNode; title: string; desc: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center gap-3.5 px-4 py-3.5">
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-border-mid">{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="text-[14px] font-medium">{title}</div>
        <div className="mt-0.5 text-[12.5px] leading-[18px] text-faint">{desc}</div>
      </div>
      {children}
    </div>
  );
}

function SecretInput({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const [shown, setShown] = useState(false);
  return (
    <span className="flex min-w-0 flex-1 items-center">
      <input
        aria-label={label}
        type={shown ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="not set"
        className="h-full min-w-0 flex-1 bg-transparent font-mono text-[13px] text-text-2 outline-none placeholder:text-faint"
      />
      <button type="button" aria-label={shown ? "Hide value" : "Show value"} onClick={() => setShown((v) => !v)} className="ml-1 text-faint hover:text-text">
        {shown ? <EyeOff size={13} /> : <Eye size={13} />}
      </button>
    </span>
  );
}

/** The active environment: Git and protection settings, then its variables. */
export function EnvironmentView() {
  const tree = useStore((s) => s.tree);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const activeId = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const { reloadTree, toast, createEnvironment } = useStore.getState();
  // Unsaved edits per environment; everything else shows what is on disk.
  const [drafts, setDrafts] = useState<Record<string, Environment>>({});
  const [busy, setBusy] = useState(false);
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

  const env = tree?.environments.find((e) => e.id === activeId) ?? null;
  const runtime = Object.entries(local?.runtimeVariables ?? {});

  if (!env) {
    return (
      <div className="flex flex-1 items-center justify-center p-8">
        <div className="max-w-[400px] text-center">
          <div className="text-[15px] font-medium">No environment</div>
          <p className="mt-1.5 text-[13px] leading-[19px] text-faint">
            Requests use collection variables only. Pick an environment on the left to use and edit it, or create one for Local, Staging or
            Production.
          </p>
          <button type="button" className="btn mt-4 h-[34px] px-4" onClick={() => void createEnvironment()}>
            <Plus size={14} /> New environment
          </button>
          {runtime.length > 0 && <RuntimeVariables workspaceId={workspaceId} entries={runtime} />}
        </div>
      </div>
    );
  }

  const saved = fromView(env);
  const draft = drafts[env.id] ?? saved;
  const dirty = stableStringify(draft) !== stableStringify(saved);
  const setDraft = (fn: (d: Environment) => Environment) => setDrafts((all) => ({ ...all, [env.id]: fn(all[env.id] ?? saved) }));
  const setVar = (i: number, patch: Partial<Variable>) =>
    setDraft((d) => ({ ...d, variables: d.variables.map((v, j) => (j === i ? { ...v, ...patch } : v)) }));
  const discard = () =>
    setDrafts((all) => {
      const next = { ...all };
      delete next[env.id];
      return next;
    });

  const act = async (fn: () => Promise<unknown>, after?: () => void) => {
    if (!workspaceId) return;
    setBusy(true);
    try {
      await fn();
      after?.();
      await reloadTree();
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const save = () => workspaceId && void act(() => api.saveEnvironment(workspaceId, draft), discard);
  const setShared = (shared: boolean) => workspaceId && void act(() => api.setEnvironmentShared(workspaceId, env.id, shared));
  const setProtected = (on: boolean) =>
    workspaceId &&
    void act(
      () => api.saveEnvironment(workspaceId, { ...saved, protected: on }),
      () => drafts[env.id] && setDraft((d) => ({ ...d, protected: on })),
    );
  const resetOverride = (key: string) => workspaceId && void act(() => api.setEnvironmentOverride(workspaceId, env.id, key, null));

  const extraOverrides = Object.entries(env.overrides).filter(([k]) => !draft.variables.some((v) => v.key === k));
  const example = draft.variables.find((v) => v.key)?.key ?? "baseUrl";
  const gitCell = (secret: boolean, local: boolean) => {
    const label = secret ? "Never synced" : local || !env.shared ? "Local only" : "Synced";
    const on = label === "Synced";
    return (
      <span className={cx("flex w-[18%] shrink-0 items-center gap-1.5 px-3 text-[12.5px]", on ? "text-ok" : "text-faint")}>
        <span className={cx("h-1.5 w-1.5 rounded-full", on ? "bg-ok" : "bg-faint")} aria-hidden />
        {label}
      </span>
    );
  };

  return (
    <section aria-label="Environment" className="min-h-0 flex-1 overflow-y-auto px-7 pt-6 pb-7">
      <div className="max-w-[760px]">
        <div className="flex items-center gap-3">
          <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: envTone(draft.name) }} aria-hidden />
          {/* The hidden copy sizes the field to its text. */}
          <span className="-mx-1 inline-grid min-w-0 text-[20px] leading-[26px] font-semibold tracking-[-0.02em]">
            <span className="invisible col-start-1 row-start-1 px-1 whitespace-pre" aria-hidden>
              {draft.name || " "}
            </span>
            <input
              aria-label="Environment name"
              size={1}
              value={draft.name}
              onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
              className="col-start-1 row-start-1 w-full min-w-[3ch] rounded-md bg-transparent px-1 outline-none hover:bg-hover focus:bg-panel-2"
            />
          </span>
          <span className="rounded bg-accent-soft px-[7px] py-0.5 text-[11.5px] font-medium text-accent-text">Active</span>
          <span className="flex-1" />
          {dirty && (
            <>
              <button type="button" className="btn-ghost h-8" onClick={discard} disabled={busy}>
                Discard
              </button>
              <button type="button" className="btn-primary h-8" onClick={save} disabled={busy || !draft.name.trim()}>
                Save
              </button>
            </>
          )}
        </div>
        <div className="mt-1 text-[13px] text-faint">
          Variables are available to every request as <span className="font-mono text-[12px] text-var">{`{{${example}}}`}</span>.
        </div>

        <div className="mt-5 divide-y divide-border rounded-[10px] border border-border-mid bg-sunken">
          <SettingCard
            icon={
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" className={env.shared ? "text-accent-text" : "text-faint"} aria-hidden>
                <circle cx="4.5" cy="3.5" r="1.5" />
                <circle cx="4.5" cy="12.5" r="1.5" />
                <circle cx="11.5" cy="6" r="1.5" />
                <path d="M4.5 5v6M11.5 7.5c0 2-2 2.5-5.5 3.5" />
              </svg>
            }
            title="Sync with Git"
            desc={
              env.shared
                ? "Variables in this environment are committed and shared with the team. Source control lists it under Changes."
                : "Variables stay on this device. This environment is left out of commits and pushes."
            }
          >
            <Switch checked={env.shared} onChange={setShared} label={`Sync ${env.name} with Git`} />
          </SettingCard>
          <SettingCard
            icon={<Lock size={15} strokeWidth={1.5} className={env.protected ? "text-warn" : "text-faint"} />}
            title="Protected"
            desc="Ask before sending requests with this environment. Agents can't run requests with it."
          >
            <Switch checked={!!env.protected} onChange={setProtected} label={`Protect ${env.name}`} />
          </SettingCard>
        </div>
        {env.protected && <div className="mt-3 text-[12.5px] text-warn">Protected environment: requests ask for confirmation before sending.</div>}

        <div className="mt-6 flex items-center justify-between">
          <span className="inline-flex items-center gap-2 text-[13px] font-semibold">
            Variables
            <span className="rounded bg-selected px-1.5 py-px text-[11px] font-medium text-muted">{draft.variables.length + extraOverrides.length}</span>
          </span>
          <button
            type="button"
            className="btn h-7 px-2.5"
            onClick={() => setDraft((d) => ({ ...d, variables: [...d.variables, { key: "", value: "", enabled: true }] }))}
          >
            <Plus size={13} strokeWidth={1.6} /> Add variable
          </button>
        </div>
        <div className="mt-2.5 overflow-hidden rounded-[10px] border border-border-mid">
          <div className="flex h-8 items-stretch border-b border-border-mid text-[13px] text-faint">
            <span className="flex w-[28%] shrink-0 items-center border-r border-border-mid px-3">Key</span>
            <span className="flex min-w-0 flex-1 items-center border-r border-border-mid px-3">Value</span>
            <span className="flex w-[18%] shrink-0 items-center px-3">Git</span>
          </div>
          {draft.variables.map((v, i) => {
            const override = env.overrides[v.key];
            return (
              <div key={i} className={cx("group flex h-9 items-stretch border-b border-border last:border-b-0", !v.enabled && "opacity-50")}>
                <span className="flex w-[28%] shrink-0 items-center border-r border-border px-3">
                  <input
                    aria-label="Key"
                    value={v.key}
                    placeholder="name"
                    onChange={(e) => setVar(i, { key: e.target.value })}
                    className="h-full min-w-0 flex-1 bg-transparent font-mono text-[13px] text-var outline-none placeholder:text-faint"
                  />
                </span>
                <span className="flex min-w-0 flex-1 items-center gap-2 border-r border-border px-3">
                  {override !== undefined ? (
                    <>
                      <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-text-2" title={`Set by a script on this device. Shared value: ${v.value || "empty"}`}>
                        {override}
                      </span>
                      <span className="text-[11px] text-faint">script</span>
                      <button type="button" aria-label="Use the shared value again" title="Use the shared value again" onClick={() => resetOverride(v.key)} className="text-faint hover:text-text">
                        <RotateCcw size={12} />
                      </button>
                    </>
                  ) : v.secret ? (
                    <>
                      <SecretInput value={v.value} onChange={(value) => setVar(i, { value })} label={`Value of ${v.key}`} />
                      <span className="text-[11px] text-faint">secret</span>
                    </>
                  ) : (
                    <input
                      aria-label={`Value of ${v.key}`}
                      value={v.value}
                      placeholder="not set"
                      onChange={(e) => setVar(i, { value: e.target.value })}
                      className="h-full min-w-0 flex-1 bg-transparent font-mono text-[13px] text-text-2 outline-none placeholder:text-faint"
                    />
                  )}
                  <MenuButton
                    title="Variable options"
                    minWidth={190}
                    className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-faint opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-text focus:opacity-100"
                    items={[
                      { label: v.secret ? "Not secret" : "Mark as secret", onSelect: () => setVar(i, { secret: !v.secret }) },
                      { label: v.enabled ? "Disable" : "Enable", onSelect: () => setVar(i, { enabled: !v.enabled }) },
                      { label: "", separator: true },
                      {
                        label: "Delete",
                        danger: true,
                        onSelect: () => setDraft((d) => ({ ...d, variables: d.variables.filter((_, j) => j !== i) })),
                      },
                    ]}
                  >
                    <MoreHorizontal size={14} />
                  </MenuButton>
                </span>
                {gitCell(!!v.secret, override !== undefined)}
              </div>
            );
          })}
          {extraOverrides.map(([k, value]) => (
            <div key={`o:${k}`} className="flex h-9 items-stretch border-b border-border last:border-b-0">
              <span className="flex w-[28%] shrink-0 items-center truncate border-r border-border px-3 font-mono text-[13px] text-var">{k}</span>
              <span className="flex min-w-0 flex-1 items-center gap-2 border-r border-border px-3">
                <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-text-2">{value}</span>
                <span className="text-[11px] text-faint">script</span>
                <button type="button" aria-label={`Remove ${k}`} title="Remove this value" onClick={() => resetOverride(k)} className="text-faint hover:text-text">
                  <RotateCcw size={12} />
                </button>
              </span>
              {gitCell(false, true)}
            </div>
          ))}
          {draft.variables.length + extraOverrides.length === 0 && <div className="px-3 py-4 text-[13px] text-faint">No variables yet.</div>}
        </div>
        <div className="mt-2.5 text-[12.5px] leading-[18px] text-faint">
          Secret values never leave this device, whatever the Git setting is. Values set by scripts (<span className="font-mono">trax.env.set</span>) stay on
          this device too.
        </div>
        {runtime.length > 0 && <RuntimeVariables workspaceId={workspaceId} entries={runtime} />}
      </div>
    </section>
  );
}

function RuntimeVariables({ workspaceId, entries }: { workspaceId: string | null; entries: [string, string][] }) {
  const { reloadTree, toast } = useStore.getState();
  return (
    <div className="mt-7 text-left">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-semibold">Session variables</span>
        <button
          type="button"
          className="btn-ghost h-7"
          onClick={async () => {
            if (!workspaceId) return;
            try {
              await api.clearRuntimeVariables(workspaceId);
              await reloadTree();
            } catch (e) {
              toast("error", errorMessage(e));
            }
          }}
        >
          Clear
        </button>
      </div>
      <p className="mt-0.5 text-[12.5px] text-faint">
        Set by scripts with <span className="font-mono">trax.vars.set</span>. They win over every environment until cleared.
      </p>
      <div className="mt-2.5 overflow-hidden rounded-[10px] border border-border-mid">
        {entries.map(([k, v]) => (
          <div key={k} className="flex h-9 items-center border-b border-border font-mono text-[13px] last:border-b-0">
            <span className="w-[28%] shrink-0 truncate px-3 text-var">{k}</span>
            <span className="min-w-0 flex-1 truncate px-3 text-text-2" title={v}>
              {v}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
