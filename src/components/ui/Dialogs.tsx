"use client";

import { AlertTriangle, Check, Copy, FolderOpen, GitBranch, Link2, Plus, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { api, errorMessage, isTauri, pickDirectory, pickFile } from "@/lib/api";
import { generate, SNIPPET_LANGS, type SnippetLang } from "@/lib/codegen";
import { parseCurl } from "@/lib/curl";
import { useStore, type WorkspaceDialogMode } from "@/lib/store";
import type { ExportRequest, GroupNode, OnboardingSpec, Request, TreeItem, WorkspaceTemplate } from "@/lib/types";
import { cx, findChain } from "@/lib/util";

import { CodeEditor } from "../editors/CodeEditor";
import { CookiesDialog } from "./CookiesDialog";
import { MethodPill } from "./Brand";
import { Switch } from "./Controls";

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

function WorkspaceDialog({ mode: initialMode }: { mode: WorkspaceDialogMode }) {
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
  const [sparse, setSparse] = useState("");
  const [templates, setTemplates] = useState<WorkspaceTemplate[]>([]);
  const [template, setTemplate] = useState("blank");
  const [link, setLink] = useState("");
  const [parsed, setParsed] = useState<{ link: string; spec: OnboardingSpec | null; error: string | null } | null>(null);
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.listWorkspaceTemplates().then(setTemplates).catch(() => undefined);
  }, []);

  // Read the onboarding link as it's pasted, to show what joining will do.
  const pasted = link.trim();
  const current = mode === "join" && parsed?.link === pasted ? parsed : null;
  const spec = current?.spec ?? null;
  useEffect(() => {
    if (mode !== "join" || !pasted) return;
    let cancelled = false;
    api
      .parseOnboardingLink(pasted)
      .then((s) => !cancelled && setParsed({ link: pasted, spec: s, error: null }))
      .catch((e) => !cancelled && setParsed({ link: pasted, spec: null, error: errorMessage(e) }));
    return () => {
      cancelled = true;
    };
  }, [mode, pasted]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const entry =
        mode === "create"
          ? await api.createWorkspace(path, name || "API workspace", team || null, initGit, template)
          : mode === "open"
            ? await api.openWorkspace(path)
            : mode === "join"
              ? await api.onboardWorkspace(link.trim(), path, secrets)
              : await api.cloneWorkspace(url, path, branch || null, subdir || null, sparseList(sparse));
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
    { id: "join", label: "Join with a link", icon: <Link2 size={14} /> },
  ];

  return (
    <Modal title="Workspaces" onClose={close} width={560}>
      <div className="tab-strip flex gap-1 px-2">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setMode(t.id)}
            className={cx(
              "flex h-10 items-center gap-1.5 border-b-2 px-3 text-[13px]",
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
            {templates.length > 0 && (
              <div>
                <label className="label" htmlFor="ws-template">
                  Start from
                </label>
                <select id="ws-template" className="input" value={template} onChange={(e) => setTemplate(e.target.value)}>
                  {templates.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name} · {t.requestCount} request{t.requestCount === 1 ? "" : "s"}
                    </option>
                  ))}
                </select>
                <p className="mt-1 text-[11.5px] text-faint">
                  {templates.find((t) => t.id === template)?.description}
                  {templates.find((t) => t.id === template)?.environments.length
                    ? ` Environments: ${templates.find((t) => t.id === template)?.environments.join(", ")}.`
                    : ""}
                </p>
              </div>
            )}
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
            <div>
              <label className="label" htmlFor="clone-sparse">
                Only check out these folders (optional)
              </label>
              <textarea
                id="clone-sparse"
                className="input h-16 py-1.5 font-mono text-[12px]"
                value={sparse}
                onChange={(e) => setSparse(e.target.value)}
                placeholder={"services/payments\nshared/api-tests"}
              />
              <p className="mt-1 text-[11.5px] text-faint">For large monorepos: a sparse checkout skips everything else. One folder per line.</p>
            </div>
          </>
        )}
        {mode === "join" && (
          <>
            <p className="text-[12.5px] text-muted">
              Paste the onboarding link a teammate shared (Settings → Team onboarding link). It clones the repository, opens the workspace and sets up the
              environment in one step.
            </p>
            <div>
              <label className="label" htmlFor="join-link">
                Onboarding link
              </label>
              <input
                id="join-link"
                className="input font-mono text-[12px]"
                value={link}
                onChange={(e) => setLink(e.target.value)}
                placeholder="courion://onboard?name=…&repo=…"
                autoFocus
              />
              {current?.error && <p className="mt-1 text-[12px] text-danger">{current.error}</p>}
            </div>
            {spec && (
              <div className="rounded-lg border border-border-mid px-3 py-2.5 text-[12.5px]">
                <div className="font-medium">{spec.name}</div>
                <dl className="mt-1 grid grid-cols-[90px_1fr] gap-x-2 gap-y-0.5 text-muted">
                  {spec.repoUrl && (
                    <>
                      <dt className="text-faint">Repository</dt>
                      <dd className="truncate font-mono text-[12px]">{spec.repoUrl}</dd>
                    </>
                  )}
                  {spec.branch && (
                    <>
                      <dt className="text-faint">Branch</dt>
                      <dd>{spec.branch}</dd>
                    </>
                  )}
                  {spec.defaultEnv && (
                    <>
                      <dt className="text-faint">Environment</dt>
                      <dd>{spec.defaultEnv}</dd>
                    </>
                  )}
                  {!!spec.sparsePatterns?.length && (
                    <>
                      <dt className="text-faint">Folders</dt>
                      <dd className="font-mono text-[12px]">{spec.sparsePatterns.join(", ")}</dd>
                    </>
                  )}
                </dl>
              </div>
            )}
            {!!spec?.requiredSecrets?.length && (
              <div>
                <div className="label">Secrets for {spec.defaultEnv ?? "the environment"} (kept on this machine)</div>
                <div className="grid grid-cols-2 gap-2">
                  {spec.requiredSecrets.map((key) => (
                    <input
                      key={key}
                      aria-label={key}
                      className="input"
                      type="password"
                      autoComplete="off"
                      placeholder={key}
                      value={secrets[key] ?? ""}
                      onChange={(e) => setSecrets({ ...secrets, [key]: e.target.value })}
                    />
                  ))}
                </div>
              </div>
            )}
            <div>
              <label className="label">{spec?.repoUrl ? "Clone into" : "Folder"}</label>
              <PathField value={path} onChange={setPath} placeholder="/home/me/work/payments-api" />
            </div>
          </>
        )}
        {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-[12.5px] text-danger">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" className="btn" onClick={close}>
            Cancel
          </button>
          <button type="submit" className="btn-primary" disabled={busy || !path || (mode === "join" && !spec)}>
            {busy ? "Working…" : mode === "create" ? "Create workspace" : mode === "open" ? "Open" : mode === "join" ? "Join" : "Clone"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** One sparse-checkout folder per line; null when there are none. */
function sparseList(text: string): string[] | null {
  const list = text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  return list.length ? list : null;
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

/** An OpenAPI or Swagger document, JSON or YAML. */
function isOpenApi(text: string): boolean {
  return /(^|[{,])\s*["']?(openapi|swagger)["']?\s*:/m.test(text.slice(0, 1024 * 1024));
}

function SpecImport({ close }: { close: () => void }) {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const { reloadTree, bumpGit, toast, setView } = useStore.getState();
  const [text, setText] = useState("");
  const [file, setFile] = useState<string | null>(null);
  // Where the picked file lives (desktop app only), so contract checks can follow it.
  const [filePath, setFilePath] = useState<string | null>(null);
  const [tests, setTests] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const openapi = isOpenApi(text);
  const chosen = file ? (
    <span className="font-mono text-[12.5px]" title={filePath ?? undefined}>
      {file}
    </span>
  ) : (
    "Choose a file (.json, .yaml, .yml, .har)"
  );
  const dropZone = "flex h-[72px] w-full cursor-pointer items-center justify-center gap-2 rounded-[10px] border border-dashed border-border-strong text-[13.5px] text-text-2 hover:bg-hover";

  return (
    <form
      className="space-y-4 p-5"
      onSubmit={async (e) => {
        e.preventDefault();
        if (!workspaceId || (!text.trim() && !filePath)) return;
        setBusy(true);
        setError(null);
        try {
          const payloadText = filePath ? null : text;
          const summary = await api.importJson(workspaceId, payloadText, filePath, openapi ? { tests, specPath: filePath ?? "" } : undefined);
          await reloadTree();
          bumpGit();
          close();
          const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
          const what = summary.collections.length
            ? `${plural(summary.requests, "request")} imported${
                summary.contract ? `, with ${plural(summary.tests, "status test")} and contract checks against ${summary.contract}` : ""
              }`
            : `Environment ${summary.environments.join(", ")} imported`;
          toast("success", summary.warnings.length ? `${what}. ${summary.warnings.length} note(s): ${summary.warnings.join(" ")}` : what);
          setView(summary.collections.length ? "collections" : "environments");
        } catch (err) {
          setError(errorMessage(err));
          setBusy(false);
        }
      }}
    >
      <p className="text-[13px] leading-[19px] text-muted">
        Import an OpenAPI 3 / Swagger spec (YAML or JSON), Postman collection or environment (v2.1), HAR archive, Insomnia export, or Bruno collection. Pick a file or paste its contents.
      </p>
      {isTauri() ? (
        <button
          type="button"
          className={dropZone}
          onClick={async () => {
            const picked = await pickFile("Choose a spec or export", ["json", "yaml", "yml", "har"]);
            if (!picked) return;
            try {
              setText(await api.readImportFile(picked));
              setFile(picked.split(/[\\/]/).pop() || picked);
              setFilePath(picked);
              setError(null);
            } catch (err) {
              setError(errorMessage(err));
            }
          }}
        >
          {chosen}
        </button>
      ) : (
        <label className={dropZone}>
          <input
            type="file"
            accept=".json,.yaml,.yml,.har,application/json,text/yaml,text/x-yaml,application/x-yaml"
            className="sr-only"
            onChange={async (e) => {
              const f = e.target.files?.[0];
              if (!f) return;
              setFile(f.name);
              setFilePath(null);
              setText(await f.text());
            }}
          />
          {chosen}
        </label>
      )}
      <div>
        <label htmlFor="spec-content" className="label">
          Or paste JSON / YAML
        </label>
        <textarea
          id="spec-content"
          value={text.length >= 1024 * 1024 && filePath ? "" : text}
          onChange={(e) => {
            setText(e.target.value);
            setFile(null);
            setFilePath(null);
          }}
          spellCheck={false}
          placeholder={
            text.length >= 1024 * 1024 && filePath
              ? "File is too large to display. Importing will work, or you can paste to replace it."
              : '{"openapi": "3.0.0", ...} or {"info": {"name": "Orders API"}, "item": [...]}'
          }
          className="input h-28 resize-none py-2.5 font-mono text-[12.5px] leading-[19px]"
        />
      </div>
      {openapi && (
        <div className="space-y-1">
          <label className="flex items-center gap-2 text-[12.5px] text-text-2">
            <input type="checkbox" checked={tests} onChange={(e) => setTests(e.target.checked)} className="accent-[var(--accent)]" />
            Generate tests
          </label>
          <p className="pl-[22px] text-[12px] leading-[17px] text-muted">
            A status check on every request, every response checked against the spec, and ids that POSTs create passed on to the requests that use them.
            {tests &&
              (filePath ? (
                <>
                  {" "}
                  Contract checks follow <span className="font-mono">{file}</span> as it changes.
                </>
              ) : (
                <>
                  {" "}
                  A copy of the spec is saved in <span className="font-mono">specs/</span> for the contract checks.
                </>
              ))}
          </p>
        </div>
      )}
      {error && <p className="text-[12.5px] text-danger">{error}</p>}
      <div className="flex justify-end gap-2">
        <button type="button" className="btn" onClick={close}>
          Cancel
        </button>
        <button type="submit" className="btn-primary" disabled={!text.trim() || busy || !workspaceId}>
          Import
        </button>
      </div>
    </form>
  );
}

function ImportDialog({ initialTab }: { initialTab?: "curl" | "postman" }) {
  const close = useStore((s) => s.closeDialog);
  const [tab, setTab] = useState<"curl" | "postman">(initialTab ?? "curl");
  return (
    <Modal title="Import" onClose={close} width={620}>
      <div className="flex items-center gap-3 px-5 pt-4">
        <div role="group" aria-label="Import from" className="segmented">
          <button type="button" aria-pressed={tab === "curl"} onClick={() => setTab("curl")}>
            cURL command
          </button>
          <button type="button" aria-pressed={tab === "postman"} onClick={() => setTab("postman")}>
            File / Spec (OpenAPI, Postman, HAR…)
          </button>
        </div>
      </div>
      {tab === "curl" ? <CurlImport close={close} /> : <SpecImport close={close} />}
    </Modal>
  );
}

function CurlImport({ close }: { close: () => void }) {
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
    <>
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
    </>
  );
}

const LANG_KEY = "http-trax.snippet-lang";

function CodeDialog({ request, parentId }: { request: Request; parentId: string | null }) {
  const close = useStore((s) => s.closeDialog);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const envId = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const [lang, setLang] = useState<SnippetLang>(() => {
    try {
      const saved = localStorage.getItem(LANG_KEY) as SnippetLang | null;
      return saved && SNIPPET_LANGS.some((l) => l.id === saved) ? saved : "curl";
    } catch {
      return "curl";
    }
  });
  const [resolve, setResolve] = useState(true);
  const [exported, setExported] = useState<{ key: string; value: ExportRequest } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const key = `${resolve}`;

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .exportRequest(workspaceId, request, parentId, envId, resolve)
      .then((value) => !cancelled && setExported({ key: `${resolve}`, value }))
      .catch((e) => !cancelled && setError(errorMessage(e)));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, request, parentId, envId, resolve]);

  const current = exported?.key === key ? exported.value : null;
  const code = current ? generate(lang, current) : "";
  const editor = SNIPPET_LANGS.find((l) => l.id === lang)?.editor ?? "text";
  const pick = (id: SnippetLang) => {
    setLang(id);
    try {
      localStorage.setItem(LANG_KEY, id);
    } catch {
      // storage unavailable
    }
  };

  return (
    <Modal title={`Code for ${request.name || "request"}`} onClose={close} width={760}>
      <div className="flex items-center justify-between gap-3 px-5 pt-4">
        <div role="group" aria-label="Language" className="segmented">
          {SNIPPET_LANGS.map((l) => (
            <button key={l.id} type="button" aria-pressed={lang === l.id} onClick={() => pick(l.id)}>
              {l.label}
            </button>
          ))}
        </div>
        <label className="flex items-center gap-2 text-[13px] text-muted">
          Fill in variables
          <Switch checked={resolve} onChange={setResolve} label="Fill in variables" />
        </label>
      </div>
      <div className="px-5 pt-3 pb-5">
        <div className="relative h-[340px] overflow-hidden rounded-[10px] border border-border-mid bg-sunken">
          {error ? (
            <p className="p-4 text-[13px] text-danger">{error}</p>
          ) : (
            <CodeEditor value={code} language={editor} readOnly lineNumbers={false} />
          )}
          <button
            type="button"
            className="btn absolute top-2 right-2 h-7 bg-elev px-2.5"
            disabled={!code}
            onClick={async () => {
              await navigator.clipboard?.writeText(code);
              setCopied(true);
              setTimeout(() => setCopied(false), 1200);
            }}
          >
            {copied ? <Check size={13} /> : <Copy size={13} strokeWidth={1.5} />}
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
        {current?.warnings.map((w) => (
          <p key={w} className="mt-2 flex items-start gap-2 text-[12.5px] text-warn">
            <AlertTriangle size={13} className="mt-0.5 shrink-0" />
            {w}
          </p>
        ))}
        <p className="mt-2 text-[12.5px] text-faint">
          {resolve
            ? "Values come from the active environment, secrets included. Share with care."
            : "Variables stay as {{placeholders}}."}
        </p>
      </div>
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
      return <ImportDialog initialTab={dialog.tab} />;
    case "code":
      return <CodeDialog request={dialog.request} parentId={dialog.parentId} />;
    case "cookies":
      return <CookiesDialog domain={dialog.domain} />;
  }
}
