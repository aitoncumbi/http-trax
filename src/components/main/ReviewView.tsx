"use client";

import { AlertTriangle, CheckCircle2, ExternalLink, GitMerge, Loader2 } from "lucide-react";
import { useMemo, useState } from "react";

import { api, errorMessage, openExternal } from "@/lib/api";
import { useStore, type ReviewTab } from "@/lib/store";
import type { MergeOutcome } from "@/lib/types";
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

/**
 * Pull-request style review of `head` against `base`: commits, changed
 * requests with line stats, the diff, and a dry-run conflict check.
 */
export function ReviewView({ tab }: { tab: ReviewTab }) {
  const { toast, bumpGit, reloadTree } = useStore.getState();
  const [selected, setSelected] = useState<string | null>(null);
  const [merging, setMerging] = useState(false);
  const review = tab.review;
  const chunks = useMemo(() => splitDiff(review?.diff ?? ""), [review]);
  const shown = selected ? (chunks.get(selected) ?? "") : (review?.diff ?? "");

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
        <button className="btn-primary" disabled={merging || review.ahead === 0} onClick={() => void mergeIntoBase()}>
          {merging ? <Loader2 size={14} className="animate-spin" /> : <GitMerge size={14} />} Merge
        </button>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-[300px_1fr]">
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
        <div className="flex min-h-0 flex-col">
          {shown ? <DiffView diff={shown} /> : <p className="p-4 text-[12.5px] text-faint">No differences.</p>}
        </div>
      </div>
    </div>
  );
}
