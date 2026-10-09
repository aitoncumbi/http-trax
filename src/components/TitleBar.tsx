"use client";

import { isTauri, windowAction } from "@/lib/api";

import { CourionMark } from "./ui/Brand";

/** 32px window strip: brand on the left, window controls on the right. Empty space drags the window. */
export function TitleBar() {
  const desktop = isTauri();
  return (
    <header className="flex h-8 shrink-0 items-center bg-frame pl-3.5 select-none" data-tauri-drag-region>
      <div className="flex items-center gap-2" data-tauri-drag-region>
        <CourionMark />
        <span className="text-[14px] font-bold tracking-[-0.02em]" data-tauri-drag-region>
          courion
        </span>
        {!desktop && (
          <span
            className="ml-1 rounded border border-border-strong px-1.5 text-[11px] leading-[18px] text-faint"
            title="Running in a browser with sample data. Install the desktop app to send real requests."
          >
            Preview
          </span>
        )}
      </div>
      <div className="h-full flex-1" data-tauri-drag-region />
      {desktop && (
        <div className="flex h-8 items-stretch">
          <button type="button" aria-label="Minimize" onClick={() => void windowAction("minimize")} className="flex w-10 items-center justify-center text-muted hover:bg-hover hover:text-text">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" aria-hidden>
              <path d="M3.5 8h9" />
            </svg>
          </button>
          <button type="button" aria-label="Maximize" onClick={() => void windowAction("toggleMaximize")} className="flex w-10 items-center justify-center text-muted hover:bg-hover hover:text-text">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" aria-hidden>
              <rect x="3.5" y="3.5" width="9" height="9" rx="1.5" />
            </svg>
          </button>
          <button type="button" aria-label="Close" onClick={() => void windowAction("close")} className="flex w-10 items-center justify-center text-muted hover:bg-[#c42b1c] hover:text-white">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" aria-hidden>
              <path d="m4 4 8 8M12 4l-8 8" />
            </svg>
          </button>
        </div>
      )}
    </header>
  );
}
