"use client";

import { Pencil, Plus, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { Identity, Remote, SyncSettings, Variable, WorkspaceManifest } from "@/lib/types";

import { KeyValueEditor, makeVar } from "../editors/KeyValueEditor";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-5">
      <h3 className="eyebrow mb-2.5">{title}</h3>
      {children}
    </section>
  );
}

export function SettingsPanel() {
  const tree = useStore((s) => s.tree);
  const info = useStore((s) => s.info);
  const entry = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId) ?? null);
  if (!entry || !tree) return null;
  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-3 pt-3 pb-6">
      <h2 className="mb-4 text-[15px] font-semibold">Settings</h2>
      <Appearance />
      {/* Keyed so the forms re-initialise when the data changes on disk. */}
      <ManifestForm key={JSON.stringify(tree.manifest)} workspaceId={entry.id} manifest={tree.manifest} root={tree.root} />
      <SyncForm key={`${entry.id}:${JSON.stringify(entry.sync)}`} workspaceId={entry.id} initial={entry.sync} />
      <GitSettings key={entry.id} workspaceId={entry.id} />
      <Section title="About">
        <p className="text-[12px] text-muted">http-trax {info?.version}</p>
        <p className="text-[12px] text-muted">{info?.gitVersion ?? "git not found — install git to sync with your team"}</p>
        <p className="font-mono text-[11px] break-all text-faint">Config: {info?.configDir}</p>
      </Section>
    </div>
  );
}

function Appearance() {
  const theme = useStore((s) => s.theme);
  const layout = useStore((s) => s.layout);
  const { toggleTheme, setLayout } = useStore.getState();
  return (
    <Section title="Appearance">
      <div className="flex items-center justify-between py-1.5">
        <span className="text-[13.5px]">Theme</span>
        <div className="segmented">
          <button type="button" aria-pressed={theme === "dark"} onClick={() => theme !== "dark" && toggleTheme()}>
            Dark
          </button>
          <button type="button" aria-pressed={theme === "light"} onClick={() => theme !== "light" && toggleTheme()}>
            Light
          </button>
        </div>
      </div>
      <div className="flex items-center justify-between py-1.5">
        <span className="text-[13.5px]">Response</span>
        <div className="segmented">
          <button type="button" aria-pressed={layout === "stack"} onClick={() => setLayout("stack")}>
            Below
          </button>
          <button type="button" aria-pressed={layout === "side"} onClick={() => setLayout("side")}>
            Beside
          </button>
        </div>
      </div>
    </Section>
  );
}

function ManifestForm({ workspaceId, manifest, root }: { workspaceId: string; manifest: WorkspaceManifest; root: string }) {
  const { toast, refreshWorkspaces, reloadTree } = useStore.getState();
  const [name, setName] = useState(manifest.name);
  const [team, setTeam] = useState(manifest.team ?? "");
  const [description, setDescription] = useState(manifest.description ?? "");
  const [variables, setVariables] = useState<Variable[]>(manifest.variables ?? []);

  const save = async () => {
    try {
      await api.updateManifest(workspaceId, { ...manifest, name, team: team || undefined, description, variables });
      await refreshWorkspaces();
      await reloadTree();
      toast("success", "Workspace saved");
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <Section title="Workspace (shared via git)">
      <label className="label">Name</label>
      <input className="input mb-2" value={name} onChange={(e) => setName(e.target.value)} />
      <label className="label">Team</label>
      <input className="input mb-2" value={team} onChange={(e) => setTeam(e.target.value)} placeholder="e.g. Payments Team" />
      <label className="label">Description</label>
      <textarea className="input mb-2 h-16 py-1.5" value={description} onChange={(e) => setDescription(e.target.value)} />
      <label className="label">Workspace variables (lowest precedence)</label>
      <KeyValueEditor rows={variables} onChange={setVariables} make={makeVar} />
      <button className="btn-primary mt-2" onClick={() => void save()}>
        <Save size={13} /> Save
      </button>
      <p className="mt-2 font-mono text-[11px] break-all text-faint">{root}</p>
    </Section>
  );
}

function SyncForm({ workspaceId, initial }: { workspaceId: string; initial: SyncSettings }) {
  const { toast, refreshWorkspaces } = useStore.getState();
  const [sync, setSync] = useState(initial);
  const save = async (next: SyncSettings) => {
    setSync(next);
    try {
      await api.updateSyncSettings(workspaceId, next);
      await refreshWorkspaces();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  return (
    <Section title="Background sync (this machine)">
      <label className="mb-1.5 flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" className="accent-[var(--accent)]" checked={sync.enabled} onChange={(e) => void save({ ...sync, enabled: e.target.checked })} />
        Fetch from the remote in the background
      </label>
      <label className="mb-2 flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" className="accent-[var(--accent)]" checked={sync.autoPull} onChange={(e) => void save({ ...sync, autoPull: e.target.checked })} />
        Fast-forward automatically when it&apos;s safe
      </label>
      <div className="grid grid-cols-2 gap-2">
        <div>
          <label className="label">Every (seconds)</label>
          <input
            className="input"
            type="number"
            min={15}
            value={sync.intervalSecs}
            onChange={(e) => setSync({ ...sync, intervalSecs: Number(e.target.value) })}
            onBlur={() => void save({ ...sync, intervalSecs: Math.max(15, sync.intervalSecs || 60) })}
          />
        </div>
        <div>
          <label className="label">Remote</label>
          <input className="input" value={sync.remote} onChange={(e) => setSync({ ...sync, remote: e.target.value })} onBlur={() => void save(sync)} />
        </div>
      </div>
      <p className="mt-2 text-[11.5px] leading-relaxed text-faint">
        Auto-pull only fast-forwards when you have no local commits and no uncommitted changes to tracked files, so it never creates merges or touches
        your edits.
      </p>
    </Section>
  );
}

function GitSettings({ workspaceId }: { workspaceId: string }) {
  const gitEpoch = useStore((s) => s.gitEpoch);
  const { toast, prompt, confirm } = useStore.getState();
  const [identity, setIdentity] = useState<Identity>({});
  const [remotes, setRemotes] = useState<Remote[]>([]);

  useEffect(() => {
    let cancelled = false;
    api.gitIdentity(workspaceId).then((id) => !cancelled && setIdentity(id)).catch(() => undefined);
    api.gitRemotes(workspaceId).then((r) => !cancelled && setRemotes(r)).catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [workspaceId, gitEpoch]);

  const reloadRemotes = async () => setRemotes(await api.gitRemotes(workspaceId));

  return (
    <>
      <Section title="Git identity for this repository">
        <p className="mb-2 text-[11.5px] text-faint">Useful when you work with several teams under different emails.</p>
        <input className="input mb-2" placeholder="Name" value={identity.name ?? ""} onChange={(e) => setIdentity({ ...identity, name: e.target.value })} />
        <input className="input mb-2" placeholder="Email" value={identity.email ?? ""} onChange={(e) => setIdentity({ ...identity, email: e.target.value })} />
        <button
          className="btn"
          onClick={async () => {
            try {
              await api.gitSetIdentity(workspaceId, identity.name ?? "", identity.email ?? "");
              toast("success", "Identity saved for this repository");
            } catch (e) {
              toast("error", errorMessage(e));
            }
          }}
        >
          <Save size={13} /> Save identity
        </button>
      </Section>

      <Section title="Remotes">
        {remotes.length === 0 && <p className="mb-2 text-[12px] text-faint">No remote. Add one to share this workspace with your team.</p>}
        {remotes.map((r) => (
          <div key={r.name} className="group mb-1.5 flex items-center gap-2 rounded-md border border-border px-2 py-1.5">
            <span className="font-semibold">{r.name}</span>
            <span className="flex-1 truncate font-mono text-[11px] text-muted" title={r.url}>
              {r.url}
            </span>
            <button
              className="icon-btn h-6 w-6"
              title="Change URL"
              onClick={async () => {
                const url = await prompt({ title: `URL for ${r.name}`, initial: r.url, confirm: "Save" });
                if (!url) return;
                try {
                  await api.gitSetRemoteUrl(workspaceId, r.name, url);
                  await reloadRemotes();
                } catch (e) {
                  toast("error", errorMessage(e));
                }
              }}
            >
              <Pencil size={12} />
            </button>
            <button
              className="icon-btn h-6 w-6"
              title="Remove remote"
              onClick={async () => {
                if (!(await confirm({ title: `Remove remote ${r.name}?`, message: "Only the remote entry is removed.", confirm: "Remove", danger: true }))) return;
                try {
                  await api.gitRemoveRemote(workspaceId, r.name);
                  await reloadRemotes();
                } catch (e) {
                  toast("error", errorMessage(e));
                }
              }}
            >
              <Trash2 size={12} />
            </button>
          </div>
        ))}
        <button
          className="btn mt-1"
          onClick={async () => {
            const url = await prompt({ title: "Add remote", label: "Repository URL", confirm: "Add" });
            if (!url) return;
            try {
              await api.gitAddRemote(workspaceId, remotes.some((r) => r.name === "origin") ? `remote${remotes.length + 1}` : "origin", url);
              await reloadRemotes();
            } catch (e) {
              toast("error", errorMessage(e));
            }
          }}
        >
          <Plus size={13} /> Add remote
        </button>
      </Section>
    </>
  );
}
