"use client";

import { Lock, Trash2 } from "lucide-react";

import { cx } from "@/lib/util";

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
  /** Show a "secret" toggle per row (environment variables). */
  secrets?: boolean;
  /** Show a text/file toggle per row (multipart bodies). */
  files?: boolean;
  /** Extra read-only info per row (e.g. local override). */
  annotate?: (row: T) => React.ReactNode;
  make: () => T;
}

/**
 * Spreadsheet-style editor for headers, params, variables and form fields.
 * There is always one empty trailing row; typing in it appends a new row.
 */
export function KeyValueEditor<T extends Row>({
  rows,
  onChange,
  keyPlaceholder = "Key",
  valuePlaceholder = "Value",
  secrets,
  files,
  annotate,
  make,
}: Props<T>) {
  const all = [...rows, make()];
  const update = (index: number, patch: Partial<T>) => {
    const next = all.map((r, i) => (i === index ? { ...r, ...patch } : r));
    // Drop the trailing row if still empty.
    const last = next[next.length - 1];
    onChange(last.key === "" && last.value === "" ? next.slice(0, -1) : next);
  };
  const remove = (index: number) => onChange(rows.filter((_, i) => i !== index));

  return (
    <div className="overflow-hidden rounded-md border border-border">
      <table className="w-full table-fixed border-collapse text-[12.5px]">
        <thead>
          <tr className="bg-panel-2 text-left text-[11px] font-semibold tracking-wide text-faint uppercase">
            <th className="w-8" />
            <th className="border-l border-border px-2 py-1.5">{keyPlaceholder}</th>
            <th className="border-l border-border px-2 py-1.5">{valuePlaceholder}</th>
            {(secrets || files) && <th className="w-16 border-l border-border" />}
            <th className="w-8" />
          </tr>
        </thead>
        <tbody>
          {all.map((row, i) => {
            const placeholder = i === rows.length;
            return (
              <tr key={i} className={cx("group border-t border-border", !row.enabled && !placeholder && "opacity-50")}>
                <td className="text-center">
                  {!placeholder && (
                    <input
                      type="checkbox"
                      aria-label="Enabled"
                      checked={row.enabled}
                      onChange={(e) => update(i, { enabled: e.target.checked } as Partial<T>)}
                      className="accent-[var(--accent)]"
                    />
                  )}
                </td>
                <td className="border-l border-border">
                  <input
                    className="h-8 w-full bg-transparent px-2 font-mono text-[12px] outline-none placeholder:text-faint"
                    placeholder={keyPlaceholder}
                    value={row.key}
                    spellCheck={false}
                    onChange={(e) => update(i, { key: e.target.value } as Partial<T>)}
                  />
                </td>
                <td className="border-l border-border">
                  <div className="flex items-center">
                    <input
                      className="h-8 w-full bg-transparent px-2 font-mono text-[12px] outline-none placeholder:text-faint"
                      placeholder={row.isFile ? "path/to/file (relative to workspace)" : valuePlaceholder}
                      type={row.secret ? "password" : "text"}
                      value={row.value}
                      spellCheck={false}
                      onChange={(e) => update(i, { value: e.target.value } as Partial<T>)}
                    />
                    {annotate && !placeholder && annotate(row)}
                  </div>
                </td>
                {(secrets || files) && (
                  <td className="border-l border-border text-center">
                    {!placeholder && secrets && (
                      <button
                        title={row.secret ? "Secret: value stays on this machine" : "Mark as secret (kept out of git)"}
                        onClick={() => update(i, { secret: !row.secret } as Partial<T>)}
                        className={cx("icon-btn", row.secret && "text-warn")}
                      >
                        <Lock size={13} />
                      </button>
                    )}
                    {!placeholder && files && (
                      <button
                        onClick={() => update(i, { isFile: !row.isFile } as Partial<T>)}
                        className="btn-ghost h-6 text-[11px]"
                        title="Toggle text / file"
                      >
                        {row.isFile ? "File" : "Text"}
                      </button>
                    )}
                  </td>
                )}
                <td className="text-center">
                  {!placeholder && (
                    <button
                      aria-label="Remove"
                      onClick={() => remove(i)}
                      className="icon-btn opacity-0 group-hover:opacity-100 focus:opacity-100"
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export const makeKv = () => ({ key: "", value: "", enabled: true });
export const makeVar = () => ({ key: "", value: "", enabled: true, secret: false });
