"use client";

import { Lock, Paperclip, Trash2 } from "lucide-react";

import { cx } from "@/lib/util";

import { Checkbox } from "../ui/Controls";
import { VarInput } from "./VarInput";

export interface Row {
  key: string;
  value: string;
  enabled: boolean;
  description?: string;
  secret?: boolean;
  isFile?: boolean;
}

interface Props<T extends Row> {
  rows: T[];
  onChange: (rows: T[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  /** Show the Description column. */
  describe?: boolean;
  /** Per-row "secret" toggle (environment variables). */
  secrets?: boolean;
  /** Per-row text/file toggle (multipart bodies). */
  files?: boolean;
  /** Extra read-only info after the value (e.g. a local override). */
  annotate?: (row: T) => React.ReactNode;
  make: () => T;
}

/** Bordered key/value table (design: "Query params"). */
export function KeyValueEditor<T extends Row>({
  rows,
  onChange,
  keyPlaceholder = "Key",
  valuePlaceholder = "Value",
  describe = false,
  secrets,
  files,
  annotate,
  make,
}: Props<T>) {
  const update = (index: number, patch: Partial<T>) => onChange(rows.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  const remove = (index: number) => onChange(rows.filter((_, i) => i !== index));
  const cell = "flex min-w-0 items-center border-r border-border";

  return (
    <div className="overflow-hidden rounded-xl border border-border-mid">
      <div className="flex h-8 items-stretch border-b border-border-mid text-[13px] text-faint">
        <span className="w-[46px] shrink-0 border-r border-border-mid" />
        <span className={cx("flex items-center border-r border-border-mid px-3", describe ? "w-[30%] shrink-0" : "w-[38%] shrink-0")}>{keyPlaceholder}</span>
        <span className={cx("flex flex-1 items-center px-3", describe && "border-r border-border-mid")}>{valuePlaceholder}</span>
        {describe && <span className="flex w-[26%] shrink-0 items-center px-3">Description</span>}
        <span className="w-9 shrink-0" />
      </div>
      {rows.map((row, i) => (
        <div key={i} className={cx("group flex h-[34px] items-stretch border-b border-border", !row.enabled && "opacity-50")}>
          <span className="flex w-[46px] shrink-0 items-center justify-center border-r border-border">
            <Checkbox checked={row.enabled} onChange={(enabled) => update(i, { enabled } as Partial<T>)} label={`Enable ${row.key || "row"}`} />
          </span>
          <span className={cx(cell, describe ? "w-[30%] shrink-0" : "w-[38%] shrink-0")}>
            <input
              aria-label={keyPlaceholder}
              className="h-full w-full bg-transparent px-3 font-mono text-[13px] text-text outline-none placeholder:text-dim"
              placeholder={keyPlaceholder}
              value={row.key}
              spellCheck={false}
              onChange={(e) => update(i, { key: e.target.value } as Partial<T>)}
            />
          </span>
          <span className={cx("flex min-w-0 flex-1 items-center", describe && "border-r border-border")}>
            {row.secret ? (
              <input
                aria-label={valuePlaceholder}
                type="password"
                className="h-full w-full bg-transparent px-3 font-mono text-[13px] text-text-2 outline-none"
                value={row.value}
                onChange={(e) => update(i, { value: e.target.value } as Partial<T>)}
              />
            ) : (
              <VarInput
                aria-label={valuePlaceholder}
                className="h-full font-mono text-[13px] text-text-2"
                placeholder={row.isFile ? "path/to/file (relative to the workspace)" : valuePlaceholder}
                value={row.value}
                onChange={(value) => update(i, { value } as Partial<T>)}
              />
            )}
            {annotate?.(row)}
            {secrets && (
              <button
                type="button"
                title={row.secret ? "Secret: the value stays on this machine" : "Mark as secret (kept out of git)"}
                aria-label="Toggle secret"
                aria-pressed={!!row.secret}
                onClick={() => update(i, { secret: !row.secret } as Partial<T>)}
                className={cx("mr-1 flex h-7 w-7 shrink-0 items-center justify-center rounded-md hover:bg-hover", row.secret ? "text-warn" : "text-dim opacity-0 group-hover:opacity-100")}
              >
                <Lock size={13} />
              </button>
            )}
            {files && (
              <button
                type="button"
                title={row.isFile ? "Sends a file" : "Send a file instead of text"}
                aria-label="Toggle file"
                aria-pressed={!!row.isFile}
                onClick={() => update(i, { isFile: !row.isFile } as Partial<T>)}
                className={cx("mr-1 flex h-7 w-7 shrink-0 items-center justify-center rounded-md hover:bg-hover", row.isFile ? "text-accent-text" : "text-dim")}
              >
                <Paperclip size={13} />
              </button>
            )}
          </span>
          {describe && (
            <span className="flex w-[26%] shrink-0 items-center">
              <input
                aria-label="Description"
                className="h-full w-full bg-transparent px-3.5 text-[13px] text-faint outline-none placeholder:text-dim"
                placeholder=""
                value={row.description ?? ""}
                onChange={(e) => update(i, { description: e.target.value } as Partial<T>)}
              />
            </span>
          )}
          <span className="flex w-9 shrink-0 items-center justify-center">
            <button
              type="button"
              aria-label={`Remove ${row.key || "row"}`}
              onClick={() => remove(i)}
              className="flex h-7 w-7 items-center justify-center rounded-md text-faint opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-text focus:opacity-100"
            >
              <Trash2 size={13} />
            </button>
          </span>
        </div>
      ))}
      <div className="p-2.5">
        <button type="button" className="btn h-7" onClick={() => onChange([...rows, make()])}>
          Add more
        </button>
      </div>
    </div>
  );
}

export const makeKv = () => ({ key: "", value: "", enabled: true, description: "" });
export const makeVar = () => ({ key: "", value: "", enabled: true, secret: false });
