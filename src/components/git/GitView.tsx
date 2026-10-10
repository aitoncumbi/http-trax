"use client";

import { AlertTriangle, ChevronDown, GitBranch, Loader2, Undo2 } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { api, errorMessage, openExternal } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { Commit, FileChanges, FileStatus, MergeOutcome, PullMode, RepoStatus, SemanticChange } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

import { Checkbox } from "../ui/Controls";
import { MenuButton, useContextMenu } from "../ui/Menu";
import { Pills, TabStrip } from "../ui/Segmented";
import { SidebarSection } from "../sidebar/Sidebar";
import { BranchPopover } from "./BranchPopover";
import { commitActions, ConflictEditor, PullRequestsPanel, SignatureBadge, StashesPanel, TagsPanel } from "./GitExtras";
import { effectiveSelection, lists, useGit } from "./gitStore";
import { RawDiff } from "./RawDiff";

function describe(outcome: MergeOutcome): string {
  switch (outcome.kind) {
    case "up-to-date":
      return "Already up to date";
    case "fast-forward":
      return `Fast-forwarded ${outcome.commits} commit${outcome.commits === 1 ? "" : "s"}`;
    case "merged":
      return "Merged";
    case "rebased":
      return "Rebased";
    case "conflicts":
      return `Conflicts in ${outcome.files.length} file${outcome.files.length === 1 ? "" : "s"}. Resolve them in Source control.`;
  }
}

/** Human name and kind for a workspace file. */
function fileLabel(path: string, prefix: string): { name: string; kind: string } {
  const rel = prefix && path.startsWith(prefix + "/") ? path.slice(prefix.length + 1) : path;
  const base = rel.split("/").pop() ?? rel;
  if (base.endsWith(".req.yaml")) return { name: base.replace(/\.req\.yaml$/, ""), kind: "request" };
  if (base.endsWith(".env.yaml")) return { name: base.replace(/\.env\.yaml$/, ""), kind: "environment" };
  if (base === "collection.yaml") return { name: rel.split("/").slice(-2, -1)[0] ?? base, kind: "collection" };
  if (base === "folder.yaml") return { name: rel.split("/").slice(-2, -1)[0] ?? base, kind: "folder" };
  if (rel.includes("responses/")) return { name: base.replace(/\.ya?ml$/, ""), kind: "response" };
  if (base === "trax.yaml") return { name: "workspace", kind: "settings" };
  return { name: base, kind: "" };
}

const TONE = {
  ok: { text: "text-ok", pill: "bg-ok/14 text-ok" },
  warn: { text: "text-warn", pill: "bg-warn/14 text-warn" },
  danger: { text: "text-danger", pill: "bg-danger/14 text-danger" },
  info: { text: "text-info", pill: "bg-info/14 text-info" },
};

function fileState(f: FileStatus, staged: boolean): { letter: string; label: string; tone: keyof typeof TONE } {
  if (f.conflicted) return { letter: "!", label: "Conflict", tone: "danger" };
  if (f.untracked) return { letter: "U", label: "New", tone: "ok" };
  switch (staged ? f.index : f.worktree) {
    case "A":
      return { letter: "A", label: "New", tone: "ok" };
    case "D":
      return { letter: "D", label: "Deleted", tone: "danger" };
    case "R":
      return { letter: "R", label: "Renamed", tone: "info" };
    default:
      return { letter: "M", label: "Modified", tone: "warn" };
  }
}

function changeTone(c: SemanticChange): keyof typeof TONE {
  return c.kind === "added" ? "ok" : c.kind === "removed" ? "danger" : "warn";
}

function ToolbarButton({
  children,
  onClick,
  disabled,
  highlight,
  title,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  highlight?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cx(
        "inline-flex h-7 items-center gap-1.5 rounded-[7px] px-2.5 text-[13px] whitespace-nowrap disabled:opacity-50",
        highlight ? "bg-accent-soft font-semibold text-accent-text-2 hover:bg-accent-soft/80" : "text-text-2 hover:bg-hover",
      )}
    >
      {children}
    </button>
  );
}

const icons = {
  fetch: (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M13 4.5v3h-3M3 11.5v-3h3" />
      <path d="M12.6 7.5A4.7 4.7 0 0 0 4 5.4M3.4 8.5A4.7 4.7 0 0 0 12 10.6" />
    </svg>
  ),
  pull: (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M8 3v9.5M4 9l4 4 4-4" />
    </svg>
  ),
  push: (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M8 13V3.5M4 7l4-4 4 4" />
    </svg>
  ),
  branch: (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden>
      <circle cx="4.5" cy="3.5" r="1.5" />
      <circle cx="4.5" cy="12.5" r="1.5" />
      <circle cx="11.5" cy="6" r="1.5" />
      <path d="M4.5 5v6M11.5 7.5c0 2-2 2.5-5.5 3.5" />
    </svg>
  ),
  check: (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="m3.5 8.3 3 3 6-6.6" />
    </svg>
  ),
};

/** Loads repository state for the active workspace while Source control is open. */
function useGitLoader(workspaceId: string | null) {
  const gitEpoch = useStore((s) => s.gitEpoch);
  const reloadKey = useGit((s) => s.reloadKey);
  useEffect(() => {
    if (workspaceId) void useGit.getState().load(workspaceId);
  }, [workspaceId, gitEpoch, reloadKey]);
}

async function discard(paths: string[], label: string) {
  const ok = await useStore.getState().confirm({
    title: paths.length === 1 ? "Discard changes?" : `Discard ${paths.length} changes?`,
    message: `${label}\n\nThis can't be undone.`,
    confirm: "Discard",
    danger: true,
  });
  if (ok) void useGit.getState().run("discard", (w) => api.gitDiscard(w, paths));
}

/** Sidebar half of Source control: changed files with stage checkboxes, and the commit box. */
export function GitChanges() {
  const status = useGit((s) => s.status);
  const log = useGit((s) => s.log);
  const busy = useGit((s) => s.busy);
  const selected = useGit((s) => s.selected);
  const message = useGit((s) => s.message);
  const { run, setSelected, setMessage, refresh } = useGit.getState();
  const { conflicted, staged, changes } = lists(status);
  const selection = effectiveSelection(selected, status, log);

  if (!status?.isRepo) {
    return (
      <SidebarSection title="Changes" />
    );
  }
  const current = status.branch ?? "HEAD";

  const commit = async (opts: { stageAll?: boolean; amend?: boolean; thenPush?: boolean }) => {
    const msg = message.trim();
    if (!msg && !opts.amend) return;
    const ok = await run(
      "commit",
      async (w) => {
        if (opts.stageAll) await api.gitStage(w, []);
        await api.gitCommit(w, msg, !!opts.amend);
        if (opts.thenPush) await api.gitPush(w);
      },
      () => (opts.thenPush ? "Committed and pushed" : opts.amend ? "Amended the last commit" : "Committed"),
    );
    if (ok) setMessage("");
  };
  const commitLabel = staged.length > 0 ? `Commit (${staged.length})` : changes.length > 0 ? `Commit all (${changes.length})` : "Commit";
  const canCommit = !!message.trim() && !busy && conflicted.length === 0 && (staged.length > 0 || changes.length > 0);

  const fileRow = (f: FileStatus, isStaged: boolean) => (
    <FileRow
      key={`${isStaged ? "s" : "u"}:${f.path}`}
      file={f}
      staged={isStaged}
      prefix={status.prefix}
      selected={selection?.kind === "file" && selection.path === f.path && (f.conflicted || selection.staged === isStaged)}
      onSelect={() => setSelected({ kind: "file", path: f.path, staged: isStaged })}
      onToggle={() =>
        void run(isStaged ? "unstage" : "stage", (w) => (isStaged ? api.gitUnstage(w, [f.path]) : api.gitStage(w, [f.path])))
      }
      onDiscard={() => void discard([f.path], f.path)}
    />
  );

  return (
    <section aria-label="Changed files" className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between pt-1.5 pl-2.5">
        <span className="inline-flex items-center gap-2 text-[13px] font-semibold">
          Changes
          <span className="rounded bg-selected px-1.5 py-px text-[11px] font-medium text-muted">{conflicted.length + staged.length + changes.length}</span>
        </span>
        <span className="flex items-center gap-1">
          {busy && <Loader2 size={13} className="animate-spin text-faint" />}
          <button type="button" aria-label="Refresh status" title="Refresh" className="icon-btn h-[26px] w-[26px]" onClick={refresh}>
            {icons.fetch}
          </button>
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pt-1 pb-2">
        {conflicted.length > 0 && (
          <>
            <SectionLabel label="CONFLICTS" count={conflicted.length} tone="text-danger" />
            {conflicted.map((f) => fileRow(f, false))}
          </>
        )}
        <SectionLabel
          label="STAGED"
          count={staged.length}
          action={staged.length > 0 ? { label: "Unstage all", onClick: () => void run("unstage", (w) => api.gitUnstage(w, [])) } : undefined}
        />
        {staged.length === 0 ? <Hint>Tick a change to include it in the next commit.</Hint> : staged.map((f) => fileRow(f, true))}
        <SectionLabel
          label="CHANGES"
          count={changes.length}
          action={
            changes.length > 0
              ? { label: "Stage all", onClick: () => void run("stage", (w) => api.gitStage(w, changes.map((c) => c.path))) }
              : undefined
          }
        />
        {changes.length === 0 ? <Hint>Nothing else changed.</Hint> : changes.map((f) => fileRow(f, false))}
      </div>
      <div className="shrink-0 border-t border-border pt-2.5">
        <label htmlFor="commit-msg" className="sr-only">
          Commit message
        </label>
        <textarea
          id="commit-msg"
          rows={3}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
              e.preventDefault();
              if (canCommit) void commit({ stageAll: staged.length === 0 });
            }
          }}
          placeholder={`Message (commit on ${current})`}
          className="block w-full resize-none rounded-[7px] border border-border-strong bg-bg px-2.5 py-2 text-[13px] leading-[18px] outline-none placeholder:text-faint focus:border-accent-line"
        />
        <div className="mt-2 flex gap-1.5">
          <button
            type="button"
            disabled={!canCommit}
            onClick={() => void commit({ stageAll: staged.length === 0 })}
            title={conflicted.length ? "Resolve conflicts first" : "Commit (Ctrl+Enter)"}
            className="inline-flex h-[30px] flex-1 items-center justify-center gap-1.5 rounded-[7px] bg-accent text-[13px] font-semibold text-white hover:bg-accent-strong disabled:opacity-50"
          >
            {icons.check}
            {commitLabel}
          </button>
          <MenuButton
            title="More commit options"
            minWidth={220}
            className="flex h-[30px] w-[30px] items-center justify-center rounded-[7px] border border-border-strong text-muted hover:bg-hover"
            items={[
              {
                label: "Commit and push",
                disabled: !canCommit || !status.hasRemote,
                onSelect: () => void commit({ stageAll: staged.length === 0, thenPush: true }),
              },
              { label: "Stage all and commit", disabled: !canCommit, onSelect: () => void commit({ stageAll: true }) },
              {
                label: "Stash changes",
                disabled: !!busy || staged.length + changes.length === 0,
                onSelect: async () => {
                  const note = await useStore.getState().prompt({ title: "Stash changes", label: "Note (optional)", initial: "WIP", confirm: "Stash" });
                  if (note !== null) void run("stash", (w) => api.gitStashPush(w, note, true), () => "Changes stashed");
                },
              },
              {
                label: "Amend last commit",
                disabled: !!busy || log.length === 0,
                onSelect: () => void commit({ amend: true }),
                hint: message.trim() ? "new message" : "keep message",
              },
              { label: "", separator: true },
              {
                label: "Discard all changes",
                danger: true,
                disabled: changes.length === 0,
                onSelect: () => void discard(changes.map((c) => c.path), "Every unstaged change in this workspace."),
              },
            ]}
          >
            <ChevronDown size={12} strokeWidth={1.8} />
          </MenuButton>
        </div>
      </div>
    </section>
  );
}

/** Main-card half of Source control: toolbar, what changed, history. */
export function GitMain() {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const gitEpoch = useStore((s) => s.gitEpoch);
  const sync = useStore((s) => (s.activeWorkspaceId ? s.sync[s.activeWorkspaceId] : undefined));
  const { toast, confirm, openReview } = useStore.getState();
  const status = useGit((s) => s.status);
  const branches = useGit((s) => s.branches);
  const log = useGit((s) => s.log);
  const busy = useGit((s) => s.busy);
  const selected = useGit((s) => s.selected);
  const reloadKey = useGit((s) => s.reloadKey);
  const { run, setSelected } = useGit.getState();
  const [branchOpen, setBranchOpen] = useState(false);
  const [aside, setAside] = useState<"history" | "stashes" | "tags" | "pulls">("history");
  useGitLoader(workspaceId);

  if (!workspaceId) return null;

  if (!status || useGit.getState().workspaceId !== workspaceId) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-[13.5px] text-muted">
        <Loader2 size={16} className="animate-spin" /> Reading the repository…
      </div>
    );
  }

  if (!status.isRepo) {
    return (
      <div className="flex flex-1 items-center justify-center p-8">
        <div className="max-w-[420px] text-center">
          <div className="mx-auto flex h-11 w-11 items-center justify-center rounded-xl border border-border-mid text-muted">
            <GitBranch size={20} strokeWidth={1.5} />
          </div>
          <h1 className="mt-4 text-[18px] font-semibold tracking-[-0.01em]">This workspace isn&apos;t in git yet</h1>
          <p className="mt-2 text-[14px] leading-[21px] text-muted">
            Start a repository to track changes to requests and environments, then add a remote to share them with your team.
          </p>
          <button
            type="button"
            className="btn-primary mt-5 h-9 px-4"
            disabled={!!busy}
            onClick={() => void run("init", (w) => api.gitInit(w), () => "Repository started")}
          >
            Start a repository
          </button>
        </div>
      </div>
    );
  }

  const { files, conflicted } = lists(status);
  const selection = effectiveSelection(selected, status, log);
  const current = status.branch ?? (status.detached ? `detached at ${status.head?.slice(0, 7) ?? "HEAD"}` : "main");
  const remoteBranches = branches.filter((b) => b.remote);
  const localBranches = branches.filter((b) => !b.remote);
  const base =
    remoteBranches.find((b) => b.name === "origin/main")?.name ??
    remoteBranches.find((b) => b.name === "origin/master")?.name ??
    localBranches.find((b) => b.name === "main" || b.name === "master")?.name ??
    "main";

  const pull = (mode: PullMode) => void run("pull", (w) => api.gitPull(w, mode), (v) => describe(v as MergeOutcome));
  const push = () => void run("push", (w) => api.gitPush(w), () => (status.upstream ? "Pushed" : `Published ${current}`));
  const selectedFile = selection?.kind === "file" ? files.find((f) => f.path === selection.path) : undefined;
  const selectedCommit = selection?.kind === "commit" ? log.find((c) => c.hash === selection.hash) : undefined;
  const fetchedAt = sync?.fetchedAt;

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <section aria-label="Review changes" className="relative flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="flex h-10 shrink-0 items-center gap-1.5 border-b border-border px-3">
          <button
            type="button"
            aria-haspopup="menu"
            aria-expanded={branchOpen}
            onClick={() => setBranchOpen((v) => !v)}
            className="inline-flex h-7 max-w-[260px] items-center gap-[7px] rounded-[7px] border border-border-strong pr-2 pl-[9px] text-[13px] font-medium hover:bg-hover"
          >
            <span className="shrink-0 text-accent-text">{icons.branch}</span>
            <span className="truncate">{current}</span>
            <ChevronDown size={11} strokeWidth={1.8} className="shrink-0 text-faint" />
          </button>
          <span className="mx-1 h-3.5 w-px bg-sep" aria-hidden />
          <ToolbarButton
            disabled={!!busy || !status.hasRemote}
            onClick={() => void run("fetch", (w) => api.gitFetch(w), () => "Fetched")}
            title={status.hasRemote ? "Fetch from the remote" : "No remote configured"}
          >
            {busy === "fetch" ? <Loader2 size={14} className="animate-spin" /> : icons.fetch}
            Fetch
          </ToolbarButton>
          <div className="flex items-center">
            <ToolbarButton disabled={!!busy || !status.upstream} onClick={() => pull("merge")} title="Pull and merge">
              {busy === "pull" ? <Loader2 size={14} className="animate-spin" /> : icons.pull}
              Pull
              <span className={cx("text-[11.5px]", status.behind ? "text-accent-text" : "text-faint")}>{status.behind}</span>
            </ToolbarButton>
            <MenuButton
              title="Pull options"
              minWidth={220}
              className="flex h-7 w-5 items-center justify-center rounded-[6px] text-faint hover:bg-hover hover:text-text"
              items={[
                { label: "Pull and merge", onSelect: () => pull("merge") },
                { label: "Pull and rebase", onSelect: () => pull("rebase") },
                { label: "Pull, fast-forward only", onSelect: () => pull("ff-only") },
              ]}
            >
              <ChevronDown size={11} strokeWidth={1.8} />
            </MenuButton>
          </div>
          <ToolbarButton
            disabled={!!busy || !status.hasRemote || status.detached}
            highlight={status.ahead > 0 || (!status.upstream && status.hasRemote)}
            onClick={push}
            title={status.upstream ? `Push to ${status.upstream}` : "Publish this branch"}
          >
            {busy === "push" ? <Loader2 size={14} className="animate-spin" /> : icons.push}
            {status.upstream || !status.hasRemote ? "Push" : "Publish"}
            {status.upstream && <span className="text-[11.5px]">{status.ahead}</span>}
          </ToolbarButton>
          <div className="flex-1" />
          <span className="truncate text-[12px] whitespace-nowrap text-faint">
            {!status.hasRemote ? "No remote · local only" : status.upstream ? `${status.upstream} · fetched ${timeAgo(fetchedAt)}` : "Not published yet"}
          </span>
        </div>

        {branchOpen && (
          <BranchPopover
            branches={branches}
            current={current}
            base={base}
            onClose={() => setBranchOpen(false)}
            actions={{
              checkout: (name) => void run("checkout", (w) => api.gitCheckout(w, name), () => `Switched to ${name}`),
              create: (name) => void run("branch", (w) => api.gitCreateBranch(w, name, null, true), () => `Created ${name}`),
              merge: async (name) => {
                const ok = await confirm({
                  title: `Merge ${name} into ${current}?`,
                  message: "If both sides changed the same request, the conflicts are listed here for you to resolve.",
                  confirm: "Merge",
                });
                if (ok) void run("merge", (w) => api.gitMerge(w, name, false), (v) => describe(v as MergeOutcome));
              },
              remove: async (name) => {
                const ok = await confirm({ title: `Delete ${name}?`, message: "Commits that are only on this branch would be lost.", confirm: "Delete", danger: true });
                if (ok) void run("delete", (w) => api.gitDeleteBranch(w, name, false), () => `Deleted ${name}`);
              },
              compare: (b) => void openReview(b, current),
              openPullRequest: async (b) => {
                try {
                  const url = await api.gitCompareUrl(workspaceId, b, current);
                  if (url) await openExternal(url);
                  else toast("info", "The remote's host isn't recognised. Open the pull request from your git host.");
                } catch (e) {
                  toast("error", errorMessage(e));
                }
              },
            }}
          />
        )}

        {(status.merging || status.rebasing) && (
          <div className="mx-4 mt-3 flex items-center gap-3 rounded-[10px] border border-danger/35 bg-danger-soft px-3.5 py-2.5 text-[13px]">
            <AlertTriangle size={15} className="shrink-0 text-danger" />
            <div className="min-w-0 flex-1">
              <div className="font-semibold text-danger">{status.rebasing ? "Rebase" : "Merge"} in progress</div>
              <div className="text-muted">
                {conflicted.length
                  ? `Resolve ${conflicted.length} conflicted file${conflicted.length === 1 ? "" : "s"}, then continue.`
                  : "All conflicts are resolved. Continue to finish."}
              </div>
            </div>
            <button type="button" className="btn h-8" disabled={!!busy} onClick={() => void run("abort", (w) => api.gitAbort(w), () => "Aborted")}>
              Abort
            </button>
            <button
              type="button"
              className="btn-primary h-8"
              disabled={!!busy || conflicted.length > 0}
              onClick={() => void run("continue", (w) => api.gitContinue(w), () => (status.rebasing ? "Rebase finished" : "Merge finished"))}
            >
              Continue
            </button>
          </div>
        )}

        {selection?.kind === "file" && selectedFile ? (
          <FileDetail
            key={`${selection.path}:${selection.staged}:${gitEpoch}:${reloadKey}`}
            workspaceId={workspaceId}
            file={selectedFile}
            staged={selection.staged && !selectedFile.conflicted}
            prefix={status.prefix}
            onResolve={(how) => void run("resolve", (w) => api.gitResolve(w, selectedFile.path, how))}
          />
        ) : selectedCommit ? (
          <CommitDetail key={selectedCommit.hash} workspaceId={workspaceId} commit={selectedCommit} current={current} busy={!!busy} />
        ) : (
          <Clean status={status} current={current} fetchedAt={fetchedAt} />
        )}
      </section>

      <aside aria-label="History" className="flex w-[276px] shrink-0 flex-col border-l border-border bg-panel">
        <TabStrip
          label="History and more"
          value={aside}
          onChange={setAside}
          tabs={[
            { id: "history", label: "History" },
            { id: "pulls", label: "PRs" },
            { id: "stashes", label: "Stashes" },
            { id: "tags", label: "Tags" },
          ]}
        />
        {aside !== "history" && (
          <div className="min-h-0 flex-1 overflow-y-auto">
            {aside === "stashes" && <StashesPanel hasChanges={files.some((f) => !f.conflicted)} />}
            {aside === "tags" && <TagsPanel hasRemote={status.hasRemote} />}
            {aside === "pulls" && <PullRequestsPanel />}
          </div>
        )}
        <div className={cx("min-h-0 flex-1 overflow-y-auto px-2 pt-2 pb-3", aside !== "history" && "hidden")}>
          {log.length === 0 && <Hint>No commits yet.</Hint>}
          {log.map((c, i) => {
            const isHead = c.refs.some((r) => r.startsWith("HEAD"));
            const sel = selection?.kind === "commit" && selection.hash === c.hash;
            const refs = c.refs.map((r) => r.replace("->", "→")).join(", ");
            return (
              <button
                type="button"
                key={c.hash}
                onClick={() => setSelected({ kind: "commit", hash: c.hash })}
                className={cx("relative flex w-full gap-2.5 rounded-[7px] p-2 text-left", sel ? "bg-selected" : "hover:bg-hover")}
              >
                <span className="relative w-3 shrink-0 self-stretch" aria-hidden>
                  {i < log.length - 1 && <span className="absolute top-3.5 -bottom-4 left-[5px] w-0.5 bg-border-mid" />}
                  <span
                    className={cx(
                      "absolute top-1 left-px h-2.5 w-2.5 rounded-full border-2 border-panel",
                      isHead ? "bg-accent-line" : c.parents.length > 1 ? "bg-purple" : "bg-checkbox-off",
                    )}
                  />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px] leading-[18px] text-text">{c.subject}</span>
                  <span className="mt-[3px] block truncate text-[11.5px] text-faint">
                    {c.authorName} · {timeAgo(c.date)} · <span className="font-mono">{c.shortHash}</span> <SignatureBadge commit={c} />
                  </span>
                  {refs && (
                    <span className="mt-[5px] inline-block max-w-full truncate rounded bg-accent-soft px-1.5 py-px text-[11px] font-medium text-accent-text-2">
                      {refs}
                    </span>
                  )}
                </span>
              </button>
            );
          })}
        </div>
      </aside>
    </div>
  );
}

function SectionLabel({
  label,
  count,
  tone,
  action,
}: {
  label: string;
  count: number;
  tone?: string;
  action?: { label: string; onClick: () => void };
}) {
  return (
    <div className="group flex h-[26px] items-end gap-1.5 px-2 pb-0.5 text-[11px] font-semibold tracking-[0.06em] text-faint">
      <span className={tone}>{label}</span>
      <span className="font-medium text-muted">{count}</span>
      {action && (
        <button
          type="button"
          onClick={action.onClick}
          className="ml-auto rounded px-1 font-medium tracking-normal text-faint opacity-0 group-hover:opacity-100 hover:text-text focus:opacity-100"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}

function Hint({ children }: { children: ReactNode }) {
  return <p className="px-2 pt-1 pb-2 text-[12px] leading-[17px] text-dim">{children}</p>;
}

function FileRow({
  file,
  staged,
  prefix,
  selected,
  onSelect,
  onToggle,
  onDiscard,
}: {
  file: FileStatus;
  staged: boolean;
  prefix: string;
  selected: boolean;
  onSelect: () => void;
  onToggle: () => void;
  onDiscard: () => void;
}) {
  const label = fileLabel(file.path, prefix);
  const st = fileState(file, staged);
  const ctx = useContextMenu(() => [
    ...(file.conflicted ? [] : [{ label: staged ? "Unstage" : "Stage", onSelect: onToggle }]),
    ...(!staged && !file.conflicted ? [{ label: "Discard changes", danger: true, onSelect: onDiscard }] : []),
    { label: "Copy path", onSelect: () => void navigator.clipboard?.writeText(file.path) },
  ]);
  return (
    <div
      onContextMenu={ctx.open}
      className={cx("group relative flex h-[34px] items-center gap-2 rounded-[7px] px-2", selected ? "bg-selected" : "hover:bg-hover")}
      title={file.path}
    >
      {selected && <span className="absolute inset-y-0 left-0 w-[3px] rounded-sm bg-accent-line" />}
      {file.conflicted ? (
        <AlertTriangle size={14} className="shrink-0 text-danger" />
      ) : (
        <Checkbox size={16} checked={staged} onChange={onToggle} label={`${staged ? "Unstage" : "Stage"} ${label.name}`} />
      )}
      <button type="button" onClick={onSelect} className={cx("flex h-full min-w-0 flex-1 items-center gap-2 text-left text-[13px]", selected ? "text-text" : "text-text-2")}>
        <span className={cx("min-w-0 flex-1 truncate", !file.inWorkspace && "text-muted")}>{label.name}</span>
        <span className="shrink-0 text-[11.5px] text-faint group-hover:hidden">{label.kind}</span>
      </button>
      {!staged && !file.conflicted && (
        <button
          type="button"
          aria-label={`Discard changes to ${label.name}`}
          title="Discard changes"
          onClick={onDiscard}
          className="hidden h-6 w-6 shrink-0 items-center justify-center rounded text-faint group-hover:flex hover:bg-selected hover:text-text"
        >
          <Undo2 size={13} />
        </button>
      )}
      <span className={cx("w-3.5 shrink-0 text-right font-mono text-[11.5px] font-semibold", TONE[st.tone].text)}>{st.letter}</span>
      {ctx.menu}
    </div>
  );
}

function FileDetail({
  workspaceId,
  file,
  staged,
  prefix,
  onResolve,
}: {
  workspaceId: string;
  file: FileStatus;
  staged: boolean;
  prefix: string;
  onResolve: (how: "ours" | "theirs" | "resolved") => void;
}) {
  const [data, setData] = useState<FileChanges | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"pretty" | "raw">("pretty");

  useEffect(() => {
    let cancelled = false;
    api
      .gitFileChanges(workspaceId, file.path, staged)
      .then((d) => !cancelled && setData(d))
      .catch((e) => !cancelled && setError(errorMessage(e)));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, file.path, staged]);

  const label = fileLabel(file.path, prefix);
  const st = fileState(file, staged);
  const semantic = data?.changes ?? null;
  const showPretty = mode === "pretty" && !file.conflicted && semantic !== null;

  return (
    <>
      <div className="flex shrink-0 items-center gap-3 px-4 pt-3 pb-2.5">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 whitespace-nowrap">
            <span className="truncate text-[15px] font-semibold tracking-[-0.01em]">{label.name}</span>
            <span className={cx("rounded px-1.5 py-0.5 text-[11.5px] font-semibold", TONE[st.tone].pill)}>{st.label}</span>
            {label.kind && <span className="text-[12px] text-faint">{label.kind}</span>}
          </div>
          <div className="mt-[3px] truncate font-mono text-[11.5px] text-faint">{file.path}</div>
        </div>
        {data && (
          <>
            <span className="font-mono text-[12px] text-ok">+{data.additions}</span>
            <span className="font-mono text-[12px] text-danger">−{data.deletions}</span>
          </>
        )}
        {!file.conflicted && semantic !== null && (
          <Pills
            options={[
              { id: "pretty", label: "Pretty" },
              { id: "raw", label: "Raw" },
            ]}
            value={mode}
            onChange={setMode}
          />
        )}
      </div>

      {file.conflicted && (
        <ConflictEditor
          workspaceId={workspaceId}
          path={file.path}
          fallback={
            <>
              <ConflictChoices onResolve={onResolve} />
              {data && (
                <div className="mx-4 mb-4 min-h-0 flex-1 overflow-auto rounded-[10px] border border-border-mid bg-sunken">
                  <RawDiff diff={data.diff} />
                </div>
              )}
            </>
          }
        />
      )}

      {error && <p className="mx-4 rounded-lg bg-danger-soft p-3 text-[13px] text-danger">{error}</p>}
      {!data && !error && !file.conflicted && (
        <div className="flex flex-1 items-center justify-center text-faint">
          <Loader2 size={16} className="animate-spin" />
        </div>
      )}

      {data && showPretty && (
        <div className="min-h-0 flex-1 overflow-auto px-4 pb-4">
          <div className="pt-0.5 pb-2.5 text-[12.5px] text-faint">
            {semantic.length === 0
              ? `Only formatting changed in ${label.name}`
              : `${semantic.length} ${semantic.length === 1 ? "change" : "changes"} in ${label.name} · ${staged ? "staged" : "not staged"}`}
          </div>
          {semantic.length > 0 && <SemanticChanges changes={semantic} />}
        </div>
      )}

      {data && !showPretty && !file.conflicted && (
        <div className="mx-4 mb-4 min-h-0 flex-1 overflow-auto rounded-[10px] border border-border-mid bg-sunken">
          <RawDiff diff={data.diff} />
        </div>
      )}
    </>
  );
}

function ConflictChoices({ onResolve }: { onResolve: (how: "ours" | "theirs" | "resolved") => void }) {
  return (
        <div className="mx-4 mb-3 flex flex-wrap items-center gap-2 rounded-[10px] border border-warn/30 bg-warn/8 px-3.5 py-2.5 text-[13px]">
          <span className="min-w-0 flex-1 text-muted">Both sides changed this file. Pick a version, or fix the file and mark it resolved.</span>
          <button type="button" className="btn h-8" onClick={() => onResolve("ours")}>
            Keep mine
          </button>
          <button type="button" className="btn h-8" onClick={() => onResolve("theirs")}>
            Take theirs
          </button>
          <button type="button" className="btn h-8" onClick={() => onResolve("resolved")} title="I fixed the file by hand">
            Mark resolved
          </button>
        </div>
  );
}

function SemanticChanges({ changes: semantic }: { changes: SemanticChange[] }) {
  return (
            <div className="overflow-hidden rounded-[10px] border border-border-mid">
              {semantic.map((c, i) => (
                <div key={i} className="border-b border-border px-3.5 py-3 last:border-b-0">
                  <div className="flex items-center gap-2.5">
                    <span className={cx("shrink-0 rounded px-[7px] py-0.5 text-[11.5px] leading-4 font-semibold", TONE[changeTone(c)].pill)}>{c.label}</span>
                    <span className="min-w-0 truncate font-mono text-[13px]">{c.subject}</span>
                    <span className="flex-1" />
                    <span className="shrink-0 text-[11.5px] text-faint">{c.area}</span>
                  </div>
                  {c.before !== null && c.before !== undefined && (
                    <div className="mt-2 flex gap-2 rounded-md bg-diff-del px-2.5 py-[5px] font-mono text-[12.5px] text-diff-del-fg">
                      <span className="w-2.5 shrink-0">−</span>
                      <span className="min-w-0 break-all whitespace-pre-wrap">{c.before}</span>
                    </div>
                  )}
                  {c.after !== null && c.after !== undefined && (
                    <div className="mt-1 flex gap-2 rounded-md bg-diff-add px-2.5 py-[5px] font-mono text-[12.5px] text-diff-add-fg">
                      <span className="w-2.5 shrink-0">+</span>
                      <span className="min-w-0 break-all whitespace-pre-wrap">{c.after}</span>
                    </div>
                  )}
                </div>
              ))}
            </div>
  );
}

function CommitDetail({ workspaceId, commit, current, busy }: { workspaceId: string; commit: Commit; current: string; busy: boolean }) {
  const [diff, setDiff] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    api
      .gitShow(workspaceId, commit.hash)
      .then((d) => !cancelled && setDiff(d))
      .catch((e) => !cancelled && setError(errorMessage(e)));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, commit.hash]);
  const files = diff ? (diff.match(/^diff --git /gm) ?? []).length : 0;
  const actions = commitActions(commit, current);
  const isHead = commit.refs.some((r) => r.startsWith("HEAD"));
  return (
    <>
      <div className="flex shrink-0 items-start gap-1.5 px-4 pt-3 pb-2.5">
        <div className="min-w-0 flex-1">
          <div className="text-[15px] font-semibold tracking-[-0.01em]">{commit.subject}</div>
          <div className="mt-[3px] text-[12px] text-faint">
            {commit.authorName} · {new Date(commit.date * 1000).toLocaleString()} · <span className="font-mono">{commit.shortHash}</span>
            {diff && ` · ${files} ${files === 1 ? "file" : "files"}`}
            {commit.parents.length > 1 && " · merge commit"}
          </div>
          {commit.signatureStatus && (
            <div className="mt-1 text-[12px] text-faint">
              <SignatureBadge commit={commit} withLabel />
              {commit.signer && <span> · signed by {commit.signer}</span>}
            </div>
          )}
        </div>
        <ToolbarButton disabled={busy || isHead} onClick={() => void actions.cherryPick()} title={isHead ? "Already on this branch" : `Apply this commit on top of ${current}`}>
          Cherry-pick
        </ToolbarButton>
        <ToolbarButton
          disabled={busy || commit.parents.length > 1}
          onClick={() => void actions.revert()}
          title={commit.parents.length > 1 ? "Merge commits can't be reverted here" : "Undo this commit with a new one"}
        >
          Revert
        </ToolbarButton>
        <ToolbarButton disabled={busy} onClick={actions.tag} title="Tag this commit">
          Tag
        </ToolbarButton>
      </div>
      {error && <p className="mx-4 rounded-lg bg-danger-soft p-3 text-[13px] text-danger">{error}</p>}
      {diff === null && !error ? (
        <div className="flex flex-1 items-center justify-center text-faint">
          <Loader2 size={16} className="animate-spin" />
        </div>
      ) : (
        diff !== null && (
          <div className="mx-4 mb-4 min-h-0 flex-1 overflow-auto rounded-[10px] border border-border-mid bg-sunken">
            <RawDiff diff={diff} showFiles />
          </div>
        )
      )}
    </>
  );
}

function Clean({ status, current, fetchedAt }: { status: RepoStatus; current: string; fetchedAt?: string | null }) {
  let line: string;
  if (!status.hasRemote) line = "There is no remote yet. Add one in Settings to share this workspace with your team.";
  else if (!status.upstream) line = `${current} isn't on the remote yet. Publish it so your team can see it.`;
  else if (status.ahead && status.behind) line = `${status.ahead} to push and ${status.behind} to pull. Pull first to merge your team's work.`;
  else if (status.ahead) line = `${status.ahead} commit${status.ahead === 1 ? "" : "s"} ready to push to ${status.upstream}.`;
  else if (status.behind) line = `${status.behind} new commit${status.behind === 1 ? "" : "s"} on ${status.upstream} to pull.`;
  else line = `Up to date with ${status.upstream}${fetchedAt ? `, checked ${timeAgo(fetchedAt)}` : ""}.`;
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-[380px] text-center">
        <div className="text-[15px] font-medium">No changes</div>
        <div className="mt-1.5 text-[13px] leading-[19px] text-faint">{line}</div>
        <div className="mt-1.5 text-[13px] leading-[19px] text-faint">Pick a commit on the right to see what it changed.</div>
      </div>
    </div>
  );
}
