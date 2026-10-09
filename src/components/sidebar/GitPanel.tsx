"use client";

import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Check,
  ChevronDown,
  ChevronRight,
  CloudDownload,
  ExternalLink,
  FileCode2,
  GitBranch,
  GitCommitHorizontal,
  GitMerge,
  GitPullRequest,
  Loader2,
  Minus,
  Plus,
  RefreshCw,
  Trash2,
  Undo2,
} from "lucide-react";
import { useEffect, useState } from "react";

import { api, errorMessage, openExternal } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { Branch, Commit, FileStatus, MergeOutcome, PullMode, RepoStatus } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

import { MenuButton, type MenuItem } from "../ui/Menu";

function describe(outcome: MergeOutcome): string {
  switch (outcome.kind) {
    case "up-to-date":
      return "Already up to date";
    case "fast-forward":
      return `Fast-forwarded ${outcome.commits} commit(s)`;
    case "merged":
      return "Merged";
    case "rebased":
      return "Rebased";
    case "conflicts":
      return `Merge conflicts in ${outcome.files.length} file(s) — resolve them below`;
  }
}

/** Human label for a workspace file. */
function fileLabel(path: string, prefix: string): { name: string; kind: string } {
  const rel = prefix && path.startsWith(prefix + "/") ? path.slice(prefix.length + 1) : path;
  const base = rel.split("/").pop() ?? rel;
  if (base.endsWith(".req.yaml")) return { name: base.replace(/\.req\.yaml$/, ""), kind: "request" };
  if (base.endsWith(".env.yaml")) return { name: base.replace(/\.env\.yaml$/, ""), kind: "environment" };
  if (base === "collection.yaml") return { name: rel.split("/").slice(-2, -1)[0] ?? base, kind: "collection" };
  if (base === "folder.yaml") return { name: rel.split("/").slice(-2, -1)[0] ?? base, kind: "folder" };
  if (rel.startsWith("responses/")) return { name: base.replace(/\.yaml$/, ""), kind: "saved response" };
  if (base === "trax.yaml") return { name: "workspace", kind: "manifest" };
  return { name: base, kind: "" };
}

function statusLetter(f: FileStatus, staged: boolean): { letter: string; tone: string } {
  if (f.conflicted) return { letter: "!", tone: "text-danger" };
  if (f.untracked) return { letter: "U", tone: "text-ok" };
  const c = staged ? f.index : f.worktree;
  switch (c) {
    case "A":
      return { letter: "A", tone: "text-ok" };
    case "D":
      return { letter: "D", tone: "text-danger" };
    case "R":
      return { letter: "R", tone: "text-info" };
    default:
      return { letter: "M", tone: "text-warn" };
  }
}

function Collapsible({
  title,
  count,
  open,
  onToggle,
  actions,
  children,
}: {
  title: string;
  count?: number;
  open: boolean;
  onToggle: () => void;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-1">
      <div className="group flex h-7 items-center gap-1 px-2">
        <button className="flex flex-1 items-center gap-1 text-left text-[11px] font-semibold tracking-wider text-faint uppercase" onClick={onToggle}>
          {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          {title}
          {count !== undefined && <span className="ml-1 rounded bg-hover px-1.5 font-mono text-[10px] text-muted">{count}</span>}
        </button>
        <div className="flex items-center opacity-70 group-hover:opacity-100">{actions}</div>
      </div>
      {open && <div>{children}</div>}
    </div>
  );
}

export function GitPanel() {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const gitEpoch = useStore((s) => s.gitEpoch);
  const sync = useStore((s) => (s.activeWorkspaceId ? s.sync[s.activeWorkspaceId] : undefined));
  const { toast, confirm, prompt, openDiff, openReview, reloadTree, bumpGit } = useStore.getState();

  const [status, setStatus] = useState<RepoStatus | null>(null);
  const [branches, setBranches] = useState<Branch[]>([]);
  const [log, setLog] = useState<Commit[]>([]);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [open, setOpen] = useState({ staged: true, changes: true, conflicts: true, branches: false, history: true });

  const [reloadKey, setReloadKey] = useState(0);
  const refresh = () => setReloadKey((k) => k + 1);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    (async () => {
      try {
        const st = await api.gitStatus(workspaceId);
        if (cancelled) return;
        setStatus(st);
        if (st.isRepo) {
          const [b, l] = await Promise.all([api.gitBranches(workspaceId), api.gitLog(workspaceId, 30)]);
          if (cancelled) return;
          setBranches(b);
          setLog(l);
        }
      } catch (e) {
        if (!cancelled) useStore.getState().toast("error", errorMessage(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceId, gitEpoch, reloadKey]);

  const run = async (label: string, fn: () => Promise<unknown>, success?: (v: unknown) => string | null) => {
    if (!workspaceId) return;
    setBusy(label);
    try {
      const v = await fn();
      const msg = success?.(v);
      if (msg) toast(v && typeof v === "object" && "kind" in v && v.kind === "conflicts" ? "error" : "success", msg);
      refresh();
      await reloadTree();
      bumpGit();
    } catch (e) {
      toast("error", errorMessage(e));
      refresh();
    } finally {
      setBusy(null);
    }
  };

  if (!workspaceId) return null;

  if (status && !status.isRepo) {
    return (
      <div className="p-3">
        <h2 className="mb-3 text-[11px] font-semibold tracking-wider text-faint uppercase">Source control</h2>
        <div className="rounded-lg border border-dashed border-border-strong p-4 text-center">
          <GitBranch size={22} className="mx-auto mb-2 text-faint" />
          <p className="text-[12.5px] text-muted">This workspace isn&apos;t in a git repository yet. Initialise one to track changes and share with your team.</p>
          <button className="btn-primary mt-3" onClick={() => void run("init", () => api.gitInit(workspaceId), () => "Repository initialised")}>
            Initialise repository
          </button>
        </div>
      </div>
    );
  }

  const files = status?.files ?? [];
  const conflicted = files.filter((f) => f.conflicted);
  const staged = files.filter((f) => f.staged && !f.conflicted);
  const changes = files.filter((f) => f.unstaged && !f.conflicted);
  const current = status?.branch ?? (status?.detached ? "(detached)" : "…");
  const localBranches = branches.filter((b) => !b.remote);
  const remoteBranches = branches.filter((b) => b.remote);
  const defaultBase =
    remoteBranches.find((b) => b.name === "origin/main")?.name ??
    remoteBranches.find((b) => b.name === "origin/master")?.name ??
    localBranches.find((b) => b.name === "main" || b.name === "master")?.name ??
    "main";

  const commit = async (stageAll: boolean) => {
    if (!message.trim()) return;
    await run(
      "commit",
      async () => {
        if (stageAll) await api.gitStage(workspaceId, []);
        return api.gitCommit(workspaceId, message.trim());
      },
      () => "Committed",
    );
    setMessage("");
  };

  const pull = (mode: PullMode) => run("pull", () => api.gitPull(workspaceId, mode), (v) => describe(v as MergeOutcome));

  const branchItems: MenuItem[] = [
    { label: "LOCAL BRANCHES", disabled: true },
    ...localBranches.map((b) => ({
      label: b.name,
      icon: b.current ? <Check size={13} className="text-accent" /> : <GitBranch size={13} />,
      hint: b.ahead || b.behind ? `↑${b.ahead} ↓${b.behind}` : undefined,
      onSelect: () => void run("checkout", () => api.gitCheckout(workspaceId, b.name), () => `Switched to ${b.name}`),
    })),
    ...(remoteBranches.length ? [{ label: "", separator: true }, { label: "REMOTE BRANCHES", disabled: true }] : []),
    ...remoteBranches.map((b) => ({
      label: b.name,
      icon: <CloudDownload size={13} />,
      onSelect: () => void run("checkout", () => api.gitCheckout(workspaceId, b.name), () => `Checked out ${b.name}`),
    })),
    { label: "", separator: true },
    {
      label: "New branch…",
      icon: <Plus size={13} />,
      onSelect: async () => {
        const name = await prompt({ title: "New branch", label: `Branch from ${current}`, confirm: "Create & switch" });
        if (name) void run("branch", () => api.gitCreateBranch(workspaceId, name, null, true), () => `Created ${name}`);
      },
    },
  ];

  const mergeItems: MenuItem[] = branches
    .filter((b) => !b.current)
    .map((b) => ({
      label: b.name,
      icon: <GitMerge size={13} />,
      onSelect: async () => {
        const ok = await confirm({ title: `Merge ${b.name} into ${current}?`, message: "Conflicts, if any, are listed in this panel for you to resolve.", confirm: "Merge" });
        if (ok) void run("merge", () => api.gitMerge(workspaceId, b.name, false), (v) => describe(v as MergeOutcome));
      },
    }));

  const reviewItems: MenuItem[] = branches
    .filter((b) => !b.current)
    .map((b) => ({
      label: `${b.name} → ${current}`,
      icon: <GitPullRequest size={13} />,
      onSelect: () => void openReview(current, b.name),
    }));

  const fileRow = (f: FileStatus, isStaged: boolean) => {
    const label = fileLabel(f.path, status?.prefix ?? "");
    const s = statusLetter(f, isStaged);
    return (
      <div key={f.path} className="group flex h-7 items-center gap-1.5 rounded-md pr-1 pl-3 hover:bg-hover">
        <button
          className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
          title={f.path}
          onClick={async () => {
            try {
              const diff = await api.gitDiff(workspaceId, f.path, isStaged);
              openDiff(`${label.name}${isStaged ? " (staged)" : ""}`, diff || "(no textual changes)");
            } catch (e) {
              toast("error", errorMessage(e));
            }
          }}
        >
          <FileCode2 size={13} className="shrink-0 text-faint" />
          <span className={cx("truncate", !f.inWorkspace && "text-muted")}>{label.name}</span>
          {label.kind && <span className="shrink-0 text-[10.5px] text-faint">{label.kind}</span>}
        </button>
        <div className="hidden items-center group-hover:flex">
          {isStaged ? (
            <button className="icon-btn h-6 w-6" title="Unstage" onClick={() => void run("unstage", () => api.gitUnstage(workspaceId, [f.path]))}>
              <Minus size={13} />
            </button>
          ) : (
            <>
              <button
                className="icon-btn h-6 w-6"
                title="Discard changes"
                onClick={async () => {
                  const ok = await confirm({ title: "Discard changes?", message: `${f.path}\n\nThis can't be undone.`, confirm: "Discard", danger: true });
                  if (ok) void run("discard", () => api.gitDiscard(workspaceId, [f.path]));
                }}
              >
                <Undo2 size={13} />
              </button>
              <button className="icon-btn h-6 w-6" title="Stage" onClick={() => void run("stage", () => api.gitStage(workspaceId, [f.path]))}>
                <Plus size={13} />
              </button>
            </>
          )}
        </div>
        <span className={cx("w-4 text-center font-mono text-[11px] font-bold", s.tone)}>{s.letter}</span>
      </div>
    );
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-3 pt-3 pb-2">
        <h2 className="text-[11px] font-semibold tracking-wider text-faint uppercase">Source control</h2>
        <div className="flex items-center">
          {busy && <Loader2 size={14} className="mr-1 animate-spin text-faint" />}
          <button className="icon-btn" title="Refresh" onClick={refresh}>
            <RefreshCw size={14} />
          </button>
        </div>
      </div>

      {/* Branch + sync */}
      <div className="mx-3 mb-2 rounded-lg border border-border bg-panel-2 p-2">
        <div className="flex items-center gap-1.5">
          <MenuButton items={branchItems} minWidth={260} className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1.5 py-1 hover:bg-hover" title="Switch branch">
            <GitBranch size={14} className="shrink-0 text-accent" />
            <span className="truncate font-semibold">{current}</span>
            <ChevronDown size={12} className="shrink-0 text-faint" />
          </MenuButton>
          {status && (status.ahead > 0 || status.behind > 0) && (
            <span className="font-mono text-[11px] text-muted">
              ↑{status.ahead} ↓{status.behind}
            </span>
          )}
        </div>
        {status?.upstream ? (
          <p className="mt-0.5 truncate px-1.5 text-[11px] text-faint">
            tracking {status.upstream} · fetched {timeAgo(sync?.fetchedAt)}
          </p>
        ) : (
          status?.hasRemote && <p className="mt-0.5 px-1.5 text-[11px] text-info">Not published yet — push to share this branch</p>
        )}
        <div className="mt-2 grid grid-cols-3 gap-1.5">
          <button className="btn h-7 px-2 text-[12px]" disabled={!!busy} onClick={() => void run("fetch", () => api.gitFetch(workspaceId), () => "Fetched")}>
            <RefreshCw size={12} /> Fetch
          </button>
          <div className="flex">
            <button
              className="btn h-7 flex-1 rounded-r-none px-2 text-[12px]"
              disabled={!!busy || !status?.hasRemote}
              onClick={() => void pull("merge")}
              title="Pull (merge)"
            >
              <ArrowDown size={12} /> Pull
            </button>
            <MenuButton
              items={[
                { label: "Pull (merge)", onSelect: () => void pull("merge") },
                { label: "Pull (rebase)", onSelect: () => void pull("rebase") },
                { label: "Pull (fast-forward only)", onSelect: () => void pull("ff-only") },
              ]}
              className="btn h-7 rounded-l-none border-l-0 px-1"
              title="Pull options"
            >
              <ChevronDown size={12} />
            </MenuButton>
          </div>
          <button
            className="btn h-7 px-2 text-[12px]"
            disabled={!!busy || !status?.hasRemote}
            onClick={() => void run("push", () => api.gitPush(workspaceId), () => "Pushed")}
          >
            <ArrowUp size={12} /> Push
          </button>
        </div>
        <div className="mt-1.5 grid grid-cols-2 gap-1.5">
          <MenuButton items={mergeItems.length ? mergeItems : [{ label: "No other branches", disabled: true }]} minWidth={240} className="btn h-7 px-2 text-[12px]" title="Merge a branch into the current one">
            <GitMerge size={12} /> Merge…
          </MenuButton>
          <MenuButton
            items={[
              { label: `Compare with ${defaultBase}`, icon: <GitPullRequest size={13} />, onSelect: () => void openReview(defaultBase, current) },
              {
                label: "Open pull request in browser",
                icon: <ExternalLink size={13} />,
                onSelect: async () => {
                  try {
                    const url = await api.gitCompareUrl(workspaceId, defaultBase, current);
                    if (url) await openExternal(url);
                    else toast("info", "The remote's host isn't recognised; open a pull request from your git host.");
                  } catch (e) {
                    toast("error", errorMessage(e));
                  }
                },
              },
              ...(reviewItems.length ? [{ label: "", separator: true }, { label: "REVIEW INCOMING BRANCH", disabled: true }, ...reviewItems] : []),
            ]}
            minWidth={280}
            className="btn h-7 px-2 text-[12px]"
            title="Review changes between branches"
          >
            <GitPullRequest size={12} /> Review…
          </MenuButton>
        </div>
      </div>

      {(status?.merging || status?.rebasing) && (
        <div className="mx-3 mb-2 rounded-lg border border-danger/40 bg-danger-soft p-2.5 text-[12px]">
          <div className="mb-1.5 flex items-center gap-1.5 font-semibold text-danger">
            <AlertTriangle size={13} /> {status.rebasing ? "Rebase" : "Merge"} in progress
          </div>
          <p className="mb-2 text-muted">
            {conflicted.length
              ? `Resolve ${conflicted.length} conflicted file(s), then continue.`
              : "All conflicts resolved. Continue to finish."}
          </p>
          <div className="flex gap-1.5">
            <button className="btn h-7 text-[12px]" onClick={() => void run("abort", () => api.gitAbort(workspaceId), () => "Aborted")}>
              Abort
            </button>
            <button
              className="btn-primary h-7 text-[12px]"
              disabled={conflicted.length > 0}
              onClick={() => void run("continue", () => api.gitContinue(workspaceId), () => "Merge completed")}
            >
              Continue
            </button>
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto pb-4">
        {/* Commit */}
        <div className="px-3 pb-2">
          <textarea
            className="input h-[58px] resize-none py-1.5"
            placeholder={`Message (commit on ${current})`}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={(e) => {
              if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
                e.preventDefault();
                void commit(staged.length === 0);
              }
            }}
          />
          <button
            className="btn-primary mt-1.5 w-full"
            disabled={!message.trim() || !!busy || (staged.length === 0 && changes.length === 0)}
            onClick={() => void commit(staged.length === 0)}
          >
            <GitCommitHorizontal size={14} />
            {staged.length === 0 && changes.length > 0 ? "Stage all & commit" : "Commit"}
          </button>
        </div>

        {conflicted.length > 0 && (
          <Collapsible title="Conflicts" count={conflicted.length} open={open.conflicts} onToggle={() => setOpen({ ...open, conflicts: !open.conflicts })}>
            {conflicted.map((f) => (
              <div key={f.path} className="px-3 py-1">
                <button
                  className="flex w-full items-center gap-1.5 text-left text-danger"
                  title={f.path}
                  onClick={async () => openDiff(fileLabel(f.path, status?.prefix ?? "").name + " (conflict)", await api.gitDiff(workspaceId, f.path, false))}
                >
                  <AlertTriangle size={12} />
                  <span className="truncate">{fileLabel(f.path, status?.prefix ?? "").name}</span>
                </button>
                <div className="mt-1 flex gap-1 pl-4">
                  <button className="btn-ghost h-6 border border-border" onClick={() => void run("resolve", () => api.gitResolve(workspaceId, f.path, "ours"))}>
                    Keep mine
                  </button>
                  <button className="btn-ghost h-6 border border-border" onClick={() => void run("resolve", () => api.gitResolve(workspaceId, f.path, "theirs"))}>
                    Take theirs
                  </button>
                  <button
                    className="btn-ghost h-6 border border-border"
                    title="I fixed the file by hand"
                    onClick={() => void run("resolve", () => api.gitResolve(workspaceId, f.path, "resolved"))}
                  >
                    Mark resolved
                  </button>
                </div>
              </div>
            ))}
          </Collapsible>
        )}

        <Collapsible
          title="Staged"
          count={staged.length}
          open={open.staged}
          onToggle={() => setOpen({ ...open, staged: !open.staged })}
          actions={
            staged.length > 0 && (
              <button className="icon-btn h-6 w-6" title="Unstage all" onClick={() => void run("unstage", () => api.gitUnstage(workspaceId, []))}>
                <Minus size={13} />
              </button>
            )
          }
        >
          {staged.length === 0 ? <p className="px-4 pb-1 text-[11.5px] text-faint">Nothing staged</p> : staged.map((f) => fileRow(f, true))}
        </Collapsible>

        <Collapsible
          title="Changes"
          count={changes.length}
          open={open.changes}
          onToggle={() => setOpen({ ...open, changes: !open.changes })}
          actions={
            changes.length > 0 && (
              <>
                <button
                  className="icon-btn h-6 w-6"
                  title="Discard all"
                  onClick={async () => {
                    const ok = await confirm({ title: "Discard all changes?", message: "Every unstaged change is lost. This can't be undone.", confirm: "Discard all", danger: true });
                    if (ok) void run("discard", () => api.gitDiscard(workspaceId, changes.map((c) => c.path)));
                  }}
                >
                  <Trash2 size={12} />
                </button>
                <button className="icon-btn h-6 w-6" title="Stage all" onClick={() => void run("stage", () => api.gitStage(workspaceId, changes.map((c) => c.path)))}>
                  <Plus size={13} />
                </button>
              </>
            )
          }
        >
          {changes.length === 0 ? <p className="px-4 pb-1 text-[11.5px] text-faint">Working tree clean</p> : changes.map((f) => fileRow(f, false))}
        </Collapsible>

        <Collapsible title="Branches" count={localBranches.length} open={open.branches} onToggle={() => setOpen({ ...open, branches: !open.branches })}>
          {localBranches.map((b) => (
            <div key={b.name} className="group flex h-7 items-center gap-1.5 rounded-md pr-1 pl-4 hover:bg-hover">
              <GitBranch size={12} className={b.current ? "text-accent" : "text-faint"} />
              <span className={cx("flex-1 truncate", b.current && "font-semibold")}>{b.name}</span>
              {(b.ahead > 0 || b.behind > 0) && <span className="font-mono text-[10.5px] text-faint">↑{b.ahead} ↓{b.behind}</span>}
              {!b.current && (
                <div className="hidden group-hover:flex">
                  <button className="icon-btn h-6 w-6" title="Review against current" onClick={() => void openReview(current, b.name)}>
                    <GitPullRequest size={12} />
                  </button>
                  <button
                    className="icon-btn h-6 w-6"
                    title="Delete branch"
                    onClick={async () => {
                      const ok = await confirm({ title: `Delete ${b.name}?`, message: "Unmerged commits on this branch would be lost.", confirm: "Delete", danger: true });
                      if (ok) void run("delete", () => api.gitDeleteBranch(workspaceId, b.name, false), () => `Deleted ${b.name}`);
                    }}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              )}
            </div>
          ))}
        </Collapsible>

        <Collapsible title="History" open={open.history} onToggle={() => setOpen({ ...open, history: !open.history })}>
          {log.length === 0 && <p className="px-4 pb-1 text-[11.5px] text-faint">No commits yet</p>}
          {log.map((c) => (
            <button
              key={c.hash}
              className="flex w-full items-start gap-2 rounded-md py-1 pr-2 pl-4 text-left hover:bg-hover"
              onClick={async () => {
                try {
                  openDiff(`${c.shortHash} ${c.subject}`, await api.gitShow(workspaceId, c.hash));
                } catch (e) {
                  toast("error", errorMessage(e));
                }
              }}
            >
              <span className={cx("mt-1.5 h-2 w-2 shrink-0 rounded-full", c.parents.length > 1 ? "bg-info" : "bg-border-strong")} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12.5px]">{c.subject}</span>
                <span className="block truncate text-[11px] text-faint">
                  {c.authorName} · {timeAgo(c.date)} · <span className="font-mono">{c.shortHash}</span>
                  {c.refs.length > 0 && <span className="text-accent"> · {c.refs.join(", ")}</span>}
                </span>
              </span>
            </button>
          ))}
        </Collapsible>
      </div>
    </div>
  );
}
