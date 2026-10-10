"use client";

import { AlertTriangle, Check, CheckCircle2, ExternalLink, GitMerge, GitPullRequest, Loader2, MessageSquare } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { api, errorMessage, openExternal } from "@/lib/api";
import { fieldLabel, fieldOptions } from "@/lib/review";
import { useStore, type ReviewTab } from "@/lib/store";
import type { MergeOutcome, PrContext, PullRequest, ReviewComment } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

import { DiffView } from "./DiffView";

/** Split a multi-file diff into per-file chunks keyed by the new path. */
function splitDiff(diff: string): Map<string, string> {
  const out = new Map<string, string>();
  const parts = diff.split(/^(?=diff --git )/m);
  for (const part of parts) {
    const m = part.match(/^diff --git a\/(.+?) b\/(.+)$/m);
    if (m) out.set(m[2], part);
  }
  return out;
}

function kindOf(path: string): string {
  if (path.endsWith(".req.yaml")) return "request";
  if (path.endsWith(".env.yaml")) return "environment";
  if (path.endsWith("collection.yaml")) return "collection";
  if (path.endsWith("folder.yaml")) return "folder";
  if (path.includes("/responses/") || path.startsWith("responses/")) return "saved response";
  return "file";
}

const stripRemote = (ref: string) => ref.replace(/^origin\//, "");

/** The PR for this review, when the remote is GitHub or GitLab. */
function usePullRequest(workspaceId: string, base: string, head: string) {
  const [ctx, setCtx] = useState<PrContext | null>(null);
  const [pr, setPr] = useState<PullRequest | null>(null);
  const [reload, setReload] = useState(0);
  useEffect(() => {
    let cancelled = false;
    api
      .getPrSettings(workspaceId)
      .then(async (c) => {
        if (cancelled) return;
        setCtx(c);
        if (!c.hosting) return;
        const open = await api.listPullRequests(workspaceId, "open");
        if (!cancelled) setPr(open.find((p) => p.sourceBranch === stripRemote(head) && p.targetBranch === stripRemote(base)) ?? null);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [workspaceId, base, head, reload]);
  return { ctx, pr, setPr, refresh: () => setReload((n) => n + 1) };
}

/**
 * Pull-request style review of `head` against `base`: commits, changed
 * requests with line stats, the diff, and a dry-run conflict check. With a
 * GitHub or GitLab remote it also creates, approves and merges the PR, and
 * holds review comments pinned to request fields.
 */
export function ReviewView({ tab }: { tab: ReviewTab }) {
  const { toast, bumpGit, reloadTree, prompt, confirm } = useStore.getState();
  const [selected, setSelected] = useState<string | null>(null);
  const [merging, setMerging] = useState(false);
  const [prefix, setPrefix] = useState("");
  const [comments, setComments] = useState<ReviewComment[]>([]);
  const { ctx, pr, setPr, refresh } = usePullRequest(tab.workspaceId, tab.base, tab.head);
  const review = tab.review;
  const chunks = useMemo(() => splitDiff(review?.diff ?? ""), [review]);
  const shown = selected ? (chunks.get(selected) ?? "") : (review?.diff ?? "");
  const provider = ctx?.hosting?.provider === "gitlab" ? "GitLab" : "GitHub";

  useEffect(() => {
    api.gitStatus(tab.workspaceId).then((st) => setPrefix(st.prefix)).catch(() => undefined);
  }, [tab.workspaceId]);

  useEffect(() => {
    let cancelled = false;
    api
      .listReviewComments(tab.workspaceId, { prNumber: pr?.number ?? null })
      .then((c) => !cancelled && setComments(c))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [tab.workspaceId, pr?.number]);

  if (tab.loading) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-muted">
        <Loader2 size={16} className="animate-spin" /> Comparing {tab.head} with {tab.base}…
      </div>
    );
  }
  if (tab.error || !review) {
    return <p className="m-4 rounded-md bg-danger-soft p-3 text-[12.5px] text-danger">{tab.error ?? "No review data"}</p>;
  }

  const conflicts = review.conflicts ?? null;
  // Comments store workspace-relative paths; the review lists repository paths.
  const repoPath = (c: ReviewComment) => (prefix ? `${prefix}/${c.path}` : c.path);
  const visible = comments.filter((c) => !selected || repoPath(c) === selected);
  const countFor = (path: string) => comments.filter((c) => !c.resolved && repoPath(c) === path).length;

  const createPr = async () => {
    const title = await prompt({ title: `Pull request for ${stripRemote(review.head)}`, label: "Title", initial: review.commits[0]?.subject ?? "", confirm: "Create" });
    if (!title) return;
    try {
      const created = await api.createPullRequest(tab.workspaceId, { title, body: "", sourceBranch: stripRemote(review.head), targetBranch: stripRemote(review.base), draft: false });
      setPr(created);
      toast("success", `Opened #${created.number} on ${provider}`);
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const approve = async () => {
    if (!pr) return;
    try {
      setPr(await api.approvePullRequest(tab.workspaceId, pr.number));
      toast("success", `Approved #${pr.number}`);
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const mergePr = async () => {
    if (!pr) return;
    if (!(await confirm({ title: `Merge #${pr.number} on ${provider}?`, message: `${pr.title}\n\n${pr.sourceBranch} is merged into ${pr.targetBranch} on the server. Pull afterwards to get it here.`, confirm: "Merge" }))) return;
    setMerging(true);
    try {
      await api.mergePullRequest(tab.workspaceId, pr.number);
      toast("success", `Merged #${pr.number}`);
      refresh();
      bumpGit();
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setMerging(false);
    }
  };
  const mergeIntoBase = async () => {
    setMerging(true);
    try {
      const status = await api.gitStatus(tab.workspaceId);
      if (status.branch !== tab.base) {
        toast("info", `Switch to ${tab.base} first, then merge ${tab.head} into it.`);
        return;
      }
      const outcome: MergeOutcome = await api.gitMerge(tab.workspaceId, tab.head, true);
      toast(outcome.kind === "conflicts" ? "error" : "success", outcome.kind === "conflicts" ? "Conflicts — resolve them in Source control" : `Merged ${tab.head} into ${tab.base}`);
      bumpGit();
      await reloadTree();
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setMerging(false);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-semibold tracking-wider text-faint uppercase">Review</div>
          <div className="truncate text-[15px] font-semibold">
            <span className="font-mono text-info">{review.head}</span> <span className="text-faint">into</span> <span className="font-mono">{review.base}</span>
          </div>
          <div className="text-[12px] text-faint">
            {review.ahead} commit{review.ahead === 1 ? "" : "s"} ahead, {review.behind} behind · {review.files.length} file{review.files.length === 1 ? "" : "s"} changed
          </div>
        </div>
        {conflicts !== null &&
          (conflicts.length > 0 ? (
            <span className="flex items-center gap-1.5 rounded-md bg-danger-soft px-2.5 py-1 text-[12px] text-danger" title={conflicts.join("\n")}>
              <AlertTriangle size={13} /> {conflicts.length} conflict{conflicts.length > 1 ? "s" : ""}
            </span>
          ) : (
            <span className="flex items-center gap-1.5 rounded-md bg-accent-soft px-2.5 py-1 text-[12px] text-accent">
              <CheckCircle2 size={13} /> Merges cleanly
            </span>
          ))}
        {pr ? (
          <>
            <button className="btn" onClick={() => void openExternal(pr.webUrl)} title={`Open #${pr.number} on ${provider}`}>
              <ExternalLink size={14} /> #{pr.number}
            </button>
            <button className="btn" disabled={pr.approved} onClick={() => void approve()}>
              <Check size={14} /> {pr.approved ? "Approved" : "Approve"}
            </button>
            <button className="btn-primary" disabled={merging} onClick={() => void mergePr()}>
              {merging ? <Loader2 size={14} className="animate-spin" /> : <GitMerge size={14} />} Merge pull request
            </button>
          </>
        ) : (
          <>
            {ctx?.hosting ? (
              <button className="btn" disabled={review.ahead === 0} onClick={() => void createPr()}>
                <GitPullRequest size={14} /> Create pull request
              </button>
            ) : (
              <button
                className="btn"
                onClick={async () => {
                  try {
                    const url = await api.gitCompareUrl(tab.workspaceId, review.base, review.head);
                    if (url) await openExternal(url);
                    else toast("info", "Couldn't build a pull request link for this remote.");
                  } catch (e) {
                    toast("error", errorMessage(e));
                  }
                }}
              >
                <ExternalLink size={14} /> Open pull request
              </button>
            )}
            <button className="btn-primary" disabled={merging || review.ahead === 0} onClick={() => void mergeIntoBase()}>
              {merging ? <Loader2 size={14} className="animate-spin" /> : <GitMerge size={14} />} Merge
            </button>
          </>
        )}
      </div>
      {pr && (
        <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-[12.5px]">
          <GitPullRequest size={13} className="text-ok" />
          <span className="min-w-0 flex-1 truncate">
            <span className="font-medium">{pr.title}</span>
            <span className="text-faint">
              {" "}
              · {pr.author} · updated {timeAgo(pr.updatedAt)}
              {pr.draft ? " · draft" : ""}
            </span>
          </span>
          {pr.approved && <span className="rounded bg-ok/14 px-1.5 py-px text-[11.5px] font-medium text-ok">Approved</span>}
        </div>
      )}
      <div className="grid min-h-0 flex-1 grid-cols-[300px_minmax(0,1fr)]">
        <aside className="min-h-0 overflow-y-auto border-r border-border">
          <div className="label px-3 pt-3">Commits</div>
          {review.commits.length === 0 && <p className="px-3 text-[12px] text-faint">None</p>}
          {review.commits.map((c) => (
            <div key={c.hash} className="px-3 py-1.5">
              <div className="truncate text-[12.5px]">{c.subject}</div>
              <div className="text-[11px] text-faint">
                {c.authorName} · {timeAgo(c.date)} · <span className="font-mono">{c.shortHash}</span>
              </div>
            </div>
          ))}
          <div className="label mt-3 px-3">Files</div>
          <button
            className={cx("flex w-full px-3 py-1 text-left text-[12px] hover:bg-hover", selected === null && "bg-accent-soft")}
            onClick={() => setSelected(null)}
          >
            All changes
          </button>
          {review.files.map((f) => (
            <button
              key={f.path}
              className={cx("flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-hover", selected === f.path && "bg-accent-soft")}
              onClick={() => setSelected(f.path)}
              title={f.path}
            >
              <span
                className={cx(
                  "w-4 font-mono text-[11px] font-bold",
                  f.status === "A" ? "text-ok" : f.status === "D" ? "text-danger" : f.status === "R" ? "text-info" : "text-warn",
                )}
              >
                {f.status}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12.5px]">{f.path.split("/").pop()}</span>
                <span className="block truncate text-[10.5px] text-faint">{kindOf(f.path)}</span>
              </span>
              {countFor(f.path) > 0 && (
                <span className="flex items-center gap-0.5 text-[10.5px] text-accent-text" title="Open comments">
                  <MessageSquare size={10} /> {countFor(f.path)}
                </span>
              )}
              <span className="font-mono text-[10.5px]">
                {f.additions !== null && f.additions !== undefined && <span className="text-ok">+{f.additions}</span>}{" "}
                {f.deletions !== null && f.deletions !== undefined && <span className="text-danger">−{f.deletions}</span>}
              </span>
            </button>
          ))}
          {conflicts && conflicts.length > 0 && (
            <>
              <div className="label mt-3 px-3 text-danger">Would conflict</div>
              {conflicts.map((c) => (
                <div key={c} className="truncate px-3 py-0.5 font-mono text-[11px] text-danger" title={c}>
                  {c}
                </div>
              ))}
            </>
          )}
        </aside>
        <div className="flex min-h-0 min-w-0 flex-col">
          <div className="flex min-h-0 flex-1 flex-col">{shown ? <DiffView diff={shown} /> : <p className="p-4 text-[12.5px] text-faint">No differences.</p>}</div>
          <ReviewComments
            key={selected ?? "all"}
            workspaceId={tab.workspaceId}
            comments={visible}
            file={selected}
            prefix={prefix}
            pr={pr}
            fields={selected ? fieldOptions(shown) : []}
            onChange={(c) => setComments((all) => (all.some((x) => x.id === c.id) ? all.map((x) => (x.id === c.id ? c : x)) : [...all, c]))}
          />
        </div>
      </div>
    </div>
  );
}

/**
 * Review comments for the selected file (or all files), each pinned to a
 * request field such as `headers.Authorization`. On a pull request they are
 * posted to GitHub or GitLab; otherwise they stay with this workspace.
 */
function ReviewComments({
  workspaceId,
  comments,
  file,
  prefix,
  pr,
  fields,
  onChange,
}: {
  workspaceId: string;
  comments: ReviewComment[];
  file: string | null;
  prefix: string;
  pr: PullRequest | null;
  fields: string[];
  onChange: (c: ReviewComment) => void;
}) {
  const { toast } = useStore.getState();
  const [field, setField] = useState("general");
  const [body, setBody] = useState("");
  const [posting, setPosting] = useState(false);
  const isRequest = !!file?.endsWith(".req.yaml");
  const wsPath = file && prefix && file.startsWith(`${prefix}/`) ? file.slice(prefix.length + 1) : file;

  const post = async () => {
    if (!wsPath || !body.trim()) return;
    setPosting(true);
    try {
      onChange(await api.addReviewComment(workspaceId, { prNumber: pr?.number ?? null, path: wsPath, field: isRequest ? field : "general", body: body.trim() }));
      setBody("");
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setPosting(false);
    }
  };

  if (!file && comments.length === 0) return null;
  return (
    <section aria-label="Review comments" className="max-h-[45%] shrink-0 overflow-y-auto border-t border-border bg-panel px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-[12.5px] font-semibold">
        <MessageSquare size={13} className="text-faint" /> Comments
        <span className="font-normal text-faint">{comments.length}</span>
      </div>
      {comments.map((c) => (
        <div key={c.id} className={cx("mb-2 rounded-[8px] border border-border-mid px-3 py-2", c.resolved && "opacity-60")}>
          <div className="flex items-center gap-2 text-[11.5px] text-faint">
            <span className="shrink-0 rounded bg-accent-soft px-1.5 py-px font-medium whitespace-nowrap text-accent-text-2">{fieldLabel(c.field)}</span>
            {!file && <span className="min-w-0 truncate font-mono">{c.requestName ?? c.path}</span>}
            <span className="min-w-0 flex-1 truncate">
              {c.author} · {timeAgo(c.createdAt)}
              {c.source !== "local" ? ` · ${c.source === "gitlab" ? "GitLab" : "GitHub"}` : ""}
            </span>
            <button
              type="button"
              className="shrink-0 hover:text-text"
              onClick={async () => {
                try {
                  onChange(await api.resolveReviewComment(workspaceId, c.id, !c.resolved));
                } catch (e) {
                  toast("error", errorMessage(e));
                }
              }}
            >
              {c.resolved ? "Reopen" : "Resolve"}
            </button>
          </div>
          <p className="mt-1 text-[13px] leading-[19px] whitespace-pre-wrap">{c.body}</p>
        </div>
      ))}
      {file && (
        <form
          className="space-y-2"
          onSubmit={(e) => {
            e.preventDefault();
            void post();
          }}
        >
          <textarea
            aria-label="Comment"
            rows={2}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={(e) => {
              if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
                e.preventDefault();
                void post();
              }
            }}
            placeholder={pr ? `Comment on #${pr.number}` : "Leave a review note"}
            className="input h-auto resize-y py-1.5"
          />
          <div className="flex items-center gap-2">
            {isRequest && (
              <>
                <label htmlFor="comment-field" className="shrink-0 text-[12px] text-faint">
                  About
                </label>
                <select id="comment-field" className="input h-8 w-auto min-w-0" value={field} onChange={(e) => setField(e.target.value)}>
                  {fields.map((f) => (
                    <option key={f} value={f}>
                      {fieldLabel(f)}
                    </option>
                  ))}
                </select>
              </>
            )}
            <span className="flex-1" />
            <button type="submit" className="btn-primary h-8 shrink-0" disabled={!body.trim() || posting}>
              {posting ? <Loader2 size={13} className="animate-spin" /> : null} Comment
            </button>
          </div>
        </form>
      )}
    </section>
  );
}
