"use client";

import { FolderOpen, GitBranch, Plus, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { api, errorMessage, isTauri, pickDirectory } from "@/lib/api";
import { useStore } from "@/lib/store";
import { cx } from "@/lib/util";

export function Modal({
  title,
  onClose,
  children,
  width = 440,
}: {
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-[12vh] backdrop-blur-[1px]"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        className="max-w-[92vw] rounded-xl border border-border-strong bg-panel shadow-[var(--shadow)]"
        style={{ width }}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label="Close">
            <X size={15} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

function PromptDialog({
  title,
  label,
  initial,
  confirm,
  resolve,
}: {
  title: string;
  label?: string;
  initial?: string;
  confirm?: string;
  resolve: (v: string | null) => void;
}) {
  const [value, setValue] = useState(initial ?? "");
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <Modal title={title} onClose={() => resolve(null)}>
      <form
        className="p-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) resolve(value.trim());
        }}
      >
        {label && <label className="label">{label}</label>}
        <input ref={ref} className="input" value={value} onChange={(e) => setValue(e.target.value)} />
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" className="btn" onClick={() => resolve(null)}>
            Cancel
          </button>
          <button type="submit" className="btn-primary" disabled={!value.trim()}>
            {confirm ?? "OK"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function ConfirmDialog({
  title,
  message,
  confirm,
  danger,
  resolve,
}: {
  title: string;
  message: string;
  confirm?: string;
  danger?: boolean;
  resolve: (v: boolean) => void;
}) {
  return (
    <Modal title={title} onClose={() => resolve(false)}>
      <div className="p-4">
        <p className="text-[13px] leading-relaxed text-muted">{message}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button className="btn" onClick={() => resolve(false)}>
            Cancel
          </button>
          <button autoFocus className={danger ? "btn-danger" : "btn-primary"} onClick={() => resolve(true)}>
            {confirm ?? "OK"}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function PathField({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder: string }) {
  return (
    <div className="flex gap-2">
      <input className="input font-mono text-[12px]" value={value} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} />
      {isTauri() && (
        <button
          type="button"
          className="btn shrink-0"
          onClick={async () => {
            const picked = await pickDirectory("Choose a folder");
            if (picked) onChange(picked);
          }}
        >
          <FolderOpen size={14} /> Browse
        </button>
      )}
    </div>
  );
}

function WorkspaceDialog({ mode: initialMode }: { mode: "create" | "open" | "clone" }) {
  const close = useStore((s) => s.closeDialog);
  const refresh = useStore((s) => s.refreshWorkspaces);
  const select = useStore((s) => s.selectWorkspace);
  const toast = useStore((s) => s.toast);
  const [mode, setMode] = useState(initialMode);
  const [name, setName] = useState("");
  const [team, setTeam] = useState("");
  const [path, setPath] = useState("");
  const [initGit, setInitGit] = useState(true);
  const [url, setUrl] = useState("");
  const [branch, setBranch] = useState("");
  const [subdir, setSubdir] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const entry =
        mode === "create"
          ? await api.createWorkspace(path, name || "API workspace", team || null, initGit)
          : mode === "open"
            ? await api.openWorkspace(path)
            : await api.cloneWorkspace(url, path, branch || null, subdir || null);
      await refresh();
      await select(entry.id);
      toast("success", `Workspace "${entry.name}" is ready`);
      close();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const tabs: { id: typeof mode; label: string; icon: ReactNode }[] = [
    { id: "create", label: "New", icon: <Plus size={14} /> },
    { id: "open", label: "Open folder", icon: <FolderOpen size={14} /> },
    { id: "clone", label: "Clone from git", icon: <GitBranch size={14} /> },
  ];

  return (
    <Modal title="Workspaces" onClose={close} width={520}>
      <div className="flex gap-1 border-b border-border px-3 pt-2">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setMode(t.id)}
            className={cx(
              "-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-[12.5px]",
              mode === t.id ? "border-accent text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>
      <form onSubmit={submit} className="space-y-3 p-4">
        {mode === "create" && (
          <>
            <p className="text-[12.5px] text-muted">
              A workspace is a folder of YAML files — put it in its own repo for a team, or inside a service repo (e.g.{" "}
              <code className="font-mono">api-tests/</code>) so tests live next to the code.
            </p>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="label">Name</label>
                <input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Payments API" required />
              </div>
              <div>
                <label className="label">Team (optional)</label>
                <input className="input" value={team} onChange={(e) => setTeam(e.target.value)} placeholder="Payments Team" />
              </div>
            </div>
            <div>
              <label className="label">Folder</label>
              <PathField value={path} onChange={setPath} placeholder="/home/me/payments-service/api-tests" />
            </div>
            <label className="flex items-center gap-2 text-[12.5px] text-muted">
              <input type="checkbox" checked={initGit} onChange={(e) => setInitGit(e.target.checked)} className="accent-[var(--accent)]" />
              Initialise a git repository if the folder isn&apos;t in one
            </label>
          </>
        )}
        {mode === "open" && (
          <>
            <p className="text-[12.5px] text-muted">Open a folder that already contains a <code className="font-mono">trax.yaml</code>.</p>
            <div>
              <label className="label">Folder</label>
              <PathField value={path} onChange={setPath} placeholder="/home/me/team-collections" />
            </div>
          </>
        )}
        {mode === "clone" && (
          <>
            <p className="text-[12.5px] text-muted">
              Clone your team&apos;s repository. Your own git credentials (SSH keys, credential manager) are used.
            </p>
            <div>
              <label className="label">Repository URL</label>
              <input className="input font-mono text-[12px]" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="git@github.com:acme/api-collections.git" required />
            </div>
            <div>
              <label className="label">Clone into</label>
              <PathField value={path} onChange={setPath} placeholder="/home/me/work/api-collections" />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="label">Branch (optional)</label>
                <input className="input" value={branch} onChange={(e) => setBranch(e.target.value)} placeholder="main" />
              </div>
              <div>
                <label className="label">Workspace subfolder (optional)</label>
                <input className="input" value={subdir} onChange={(e) => setSubdir(e.target.value)} placeholder="api-tests" />
              </div>
            </div>
          </>
        )}
        {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-[12.5px] text-danger">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" className="btn" onClick={close}>
            Cancel
          </button>
          <button type="submit" className="btn-primary" disabled={busy || !path}>
            {busy ? "Working…" : mode === "create" ? "Create workspace" : mode === "open" ? "Open" : "Clone"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function Dialogs() {
  const dialog = useStore((s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case "prompt":
      return <PromptDialog {...dialog} />;
    case "confirm":
      return <ConfirmDialog {...dialog} />;
    case "workspace":
      return <WorkspaceDialog mode={dialog.mode} />;
  }
}
