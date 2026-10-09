"use client";

import { FolderOpen, GitBranch, Plus, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { api, errorMessage, isTauri, pickDirectory } from "@/lib/api";
import { parseCurl } from "@/lib/curl";
import { useStore } from "@/lib/store";
import type { GroupNode, TreeItem } from "@/lib/types";
import { cx, findChain } from "@/lib/util";

import { MethodPill } from "./Brand";

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
      className="fixed inset-0 z-50 flex items-start justify-center bg-[var(--scrim)] pt-[12vh] backdrop-blur-[4px]"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        className="max-w-[calc(100vw-32px)] overflow-hidden rounded-[14px] border border-border-strong bg-elev shadow-[var(--shadow)]"
        style={{ width }}
      >
        <div className="flex h-[52px] items-center justify-between border-b border-border pr-3 pl-5">
          <h2 className="text-[15px] font-semibold tracking-[-0.01em]">{title}</h2>
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
        className="p-5"
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
      <div className="p-5">
        <p className="text-[13.5px] leading-relaxed whitespace-pre-wrap text-muted">{message}</p>
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
      <div className="flex gap-1 border-b border-border px-4">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setMode(t.id)}
            className={cx(
              "-mb-px flex h-10 items-center gap-1.5 border-b-2 px-3 text-[13px]",
              mode === t.id ? "border-accent-line font-medium text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>
      <form onSubmit={submit} className="space-y-4 p-5">
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

function groupOptions(collections: GroupNode[]): { id: string; label: string }[] {
  const out: { id: string; label: string }[] = [];
  const walk = (node: GroupNode, trail: string[]) => {
    const here = [...trail, node.meta.name];
    out.push({ id: node.meta.id, label: here.join(" / ") });
    node.items.forEach((i: TreeItem) => i.kind === "folder" && walk(i, here));
  };
  collections.forEach((c) => walk(c, []));
  return out;
}

function ImportDialog() {
  const close = useStore((s) => s.closeDialog);
  const tree = useStore((s) => s.tree);
  const activeTab = useStore((s) => s.tabs.find((t) => t.id === (s.activeWorkspaceId ? s.activeTab[s.activeWorkspaceId] : null)));
  const importRequest = useStore((s) => s.importRequest);
  const options = groupOptions(tree?.collections ?? []);
  const defaultParent = (() => {
    if (activeTab?.kind === "request") {
      const chain = findChain(tree?.collections ?? [], activeTab.requestId);
      if (chain?.length) return chain[chain.length - 1].meta.id;
    }
    return options[0]?.id ?? "";
  })();
  const [text, setText] = useState("");
  const [parent, setParent] = useState(defaultParent);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  let preview: ReturnType<typeof parseCurl> | null = null;
  let error: string | null = null;
  if (text.trim()) {
    try {
      preview = parseCurl(text);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  return (
    <Modal title="Import from cURL" onClose={close} width={620}>
      <form
        className="space-y-4 p-5"
        onSubmit={async (e) => {
          e.preventDefault();
          if (!preview || !parent) return;
          setBusy(true);
          const request = { ...preview.request, name: name.trim() || preview.request.name };
          close();
          await importRequest(parent, request);
        }}
      >
        <div>
          <label htmlFor="curl-text" className="label">
            Paste a cURL command
          </label>
          <textarea
            id="curl-text"
            autoFocus
            value={text}
            onChange={(e) => setText(e.target.value)}
            spellCheck={false}
            placeholder={"curl -X POST https://api.example.com/v1/orders \\\n  -H 'Content-Type: application/json' \\\n  -d '{\"sku\": \"TRX-001\"}'"}
            className="input h-36 resize-none py-2.5 font-mono text-[12.5px] leading-[19px]"
          />
        </div>
        {preview && (
          <div className="flex items-center gap-3 rounded-lg border border-border-mid px-3 py-2.5">
            <MethodPill method={preview.request.method} />
            <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-text-2">{preview.request.url}</span>
            <span className="text-[12px] text-faint">
              {(preview.request.headers ?? []).length} headers · {preview.request.body?.mode === "none" ? "no body" : preview.request.body?.mode}
            </span>
          </div>
        )}
        {preview && preview.warnings.length > 0 && <p className="text-[12.5px] text-warn">{preview.warnings.join(", ")} (not supported).</p>}
        {error && <p className="text-[12.5px] text-danger">{error}</p>}
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label htmlFor="curl-name" className="label">
              Name
            </label>
            <input id="curl-name" className="input" value={name} placeholder={preview?.request.name ?? "Request name"} onChange={(e) => setName(e.target.value)} />
          </div>
          <div>
            <label htmlFor="curl-parent" className="label">
              Save to
            </label>
            <select id="curl-parent" className="input" value={parent} onChange={(e) => setParent(e.target.value)}>
              {options.length === 0 && <option value="">Create a collection first</option>}
              {options.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button type="button" className="btn" onClick={close}>
            Cancel
          </button>
          <button type="submit" className="btn-primary" disabled={!preview || !parent || busy}>
            Import request
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
    case "import":
      return <ImportDialog />;
  }
}
