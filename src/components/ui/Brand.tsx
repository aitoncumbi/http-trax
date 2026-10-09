import { avatarTone, cx, initials, methodLabel, methodPill } from "@/lib/util";

/** The Courion mark (from the design canvas). */
export function CourionMark({ size = 22 }: { size?: number }) {
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center rounded-md bg-accent"
      style={{ width: size, height: size }}
    >
      <svg width={size * 0.77} height={size * 0.77} viewBox="0 0 24 24">
        <g fill="#ffffff">
          <ellipse cx="12" cy="14" rx="6.6" ry="4.3" transform="rotate(-20 12 14)" />
          <circle cx="16.8" cy="9.6" r="3.1" />
          <path d="M19.5 8.9 22.4 9.8 19.7 10.9z" />
          <path d="M6.6 15.4 2 18.8 2.4 14.4 6.2 12.8z" />
        </g>
        <path d="M10.6 18.2v2.3M13.4 17.8v2.5" fill="none" stroke="#ffffff" strokeWidth="1.3" strokeLinecap="round" />
        <circle cx="17.6" cy="9.1" r="0.6" fill="#2f6df6" />
        <path d="M7.8 13.4Q11.4 16.2 15.4 12.8" fill="none" stroke="#2f6df6" strokeWidth="1.1" strokeLinecap="round" />
      </svg>
    </span>
  );
}

export function MethodPill({ method, className }: { method: string; className?: string }) {
  return (
    <span
      className={cx(
        "inline-block min-w-[34px] shrink-0 rounded px-[5px] py-[2px] text-center font-mono text-[10px] leading-[13px] font-semibold tracking-[0.02em]",
        methodPill(method),
        className,
      )}
    >
      {methodLabel(method)}
    </span>
  );
}

export function WorkspaceAvatar({ id, name, size = 20, local }: { id: string; name: string; size?: number; local?: boolean }) {
  const tone = avatarTone(id);
  if (local) {
    return (
      <span
        aria-hidden
        className="inline-flex shrink-0 items-center justify-center rounded-[9px] bg-selected text-muted"
        style={{ width: size, height: size }}
      >
        <svg width={size * 0.47} height={size * 0.47} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round">
          <rect x="2.5" y="4" width="11" height="8" rx="1.8" />
          <path d="M5.5 14h5" />
        </svg>
      </span>
    );
  }
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center font-bold"
      style={{
        width: size,
        height: size,
        borderRadius: size > 24 ? 9 : 5,
        background: tone.bg,
        color: tone.fg,
        fontSize: size > 24 ? 12 : 10,
      }}
    >
      {initials(name)}
    </span>
  );
}
