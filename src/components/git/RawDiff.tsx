"use client";

import { cx } from "@/lib/util";

type Row =
  | { type: "file"; text: string }
  | { type: "hunk"; text: string; first: boolean }
  | { type: "line"; sign: "+" | "-" | " "; a: number | null; b: number | null; text: string; conflict: boolean }
  | { type: "note"; text: string };

const META = /^(index |--- |\+\+\+ |new file|deleted file|similarity|dissimilarity|rename |copy |old mode|new mode|mode )/;
const CONFLICT = /^(<<<<<<<|=======|>>>>>>>|\|\|\|\|\|\|\|)/;

/** Unified (or combined, for conflicts) diff to rows with old/new line numbers. */
export function parseDiff(diff: string): Row[] {
  const rows: Row[] = [];
  let a = 0;
  let b = 0;
  let width = 1; // number of sign columns: 1 for normal diffs, 2 for `diff --cc`
  let inFile = false;
  let firstHunk = true;
  for (const line of diff.split("\n")) {
    if (line.startsWith("diff --git ") || line.startsWith("diff --cc ") || line.startsWith("diff --combined ")) {
      width = line.startsWith("diff --git ") ? 1 : 2;
      const m = line.match(/^diff --git a\/.+? b\/(.+)$/) ?? line.match(/^diff --(?:cc|combined) (.+)$/);
      rows.push({ type: "file", text: m?.[1] ?? line });
      inFile = true;
      firstHunk = true;
      continue;
    }
    if (!inFile) continue;
    if (META.test(line)) continue;
    if (line.startsWith("Binary files")) {
      rows.push({ type: "note", text: "Binary file changed" });
      continue;
    }
    if (line.startsWith("@@")) {
      const nums = [...line.matchAll(/[-+](\d+)(?:,\d+)?/g)].map((m) => Number(m[1]));
      a = (nums[0] ?? 1) - 1;
      b = (nums[nums.length - 1] ?? 1) - 1;
      const context = line.replace(/^@@+[^@]*@@+\s?/, "");
      rows.push({ type: "hunk", text: context, first: firstHunk });
      firstHunk = false;
      continue;
    }
    if (line.startsWith("\\")) {
      rows.push({ type: "note", text: line.slice(2) });
      continue;
    }
    const signs = line.slice(0, width);
    const text = line.slice(width);
    if (line === "" && width === 1) continue;
    const conflict = CONFLICT.test(text);
    if (signs.includes("+")) {
      b++;
      rows.push({ type: "line", sign: "+", a: null, b, text, conflict });
    } else if (signs.includes("-")) {
      a++;
      rows.push({ type: "line", sign: "-", a, b: null, text, conflict });
    } else {
      a++;
      b++;
      rows.push({ type: "line", sign: " ", a, b, text, conflict });
    }
  }
  return rows;
}

export function RawDiff({ diff, showFiles = false }: { diff: string; showFiles?: boolean }) {
  const rows = parseDiff(diff);
  if (rows.filter((r) => r.type !== "file").length === 0) {
    return <div className="px-4 py-6 text-center text-[13px] text-faint">No line changes.</div>;
  }
  return (
    <div className="min-w-max py-1.5 font-mono text-[12.5px] leading-5">
      {rows.map((r, i) => {
        if (r.type === "file") {
          return showFiles ? (
            <div key={i} className="sticky left-0 mt-2 border-y border-border bg-panel px-3 py-1.5 font-sans text-[12.5px] font-semibold text-text first:mt-0 first:border-t-0">
              {r.text}
            </div>
          ) : null;
        }
        if (r.type === "hunk") {
          if (r.first && !r.text) return null;
          return (
            <div key={i} className={cx("flex text-dim select-none", !r.first && "mt-1 border-t border-dashed border-border")}>
              <span className="w-[98px] shrink-0 pr-2 text-right">⋯</span>
              <span className="truncate pl-1">{r.text}</span>
            </div>
          );
        }
        if (r.type === "note") {
          return (
            <div key={i} className="pl-[98px] text-[12px] text-faint italic">
              {r.text}
            </div>
          );
        }
        const tone = r.conflict
          ? "bg-warn/12 text-warn"
          : r.sign === "+"
            ? "bg-diff-add text-diff-add-fg"
            : r.sign === "-"
              ? "bg-diff-del text-diff-del-fg"
              : "text-muted";
        return (
          <div key={i} className={cx("flex", tone)}>
            <span className="w-10 shrink-0 pr-2 text-right text-dim select-none">{r.a ?? ""}</span>
            <span className="w-10 shrink-0 pr-2 text-right text-dim select-none">{r.b ?? ""}</span>
            <span className="w-[18px] shrink-0 text-center select-none">{r.sign === "-" ? "−" : r.sign === "+" ? "+" : ""}</span>
            <span className="pr-4 whitespace-pre">{r.text}</span>
          </div>
        );
      })}
    </div>
  );
}
