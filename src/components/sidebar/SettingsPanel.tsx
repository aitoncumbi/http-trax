"use client";

import { SETTINGS_SECTIONS } from "../main/SettingsView";
import { SidebarSection } from "./Sidebar";

/** Settings sections; the page itself is in the main card. */
export function SettingsPanel() {
  return (
    <nav aria-label="Settings sections" className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <SidebarSection title="Settings" />
      {SETTINGS_SECTIONS.map((s) => (
        <button
          key={s.id}
          type="button"
          onClick={() => document.getElementById(`settings-${s.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" })}
          className="flex h-8 items-center rounded-[7px] px-2.5 text-left text-[13.5px] text-text-2 hover:bg-hover hover:text-text"
        >
          {s.title.replace(/ \(.*\)$/, "")}
        </button>
      ))}
    </nav>
  );
}
