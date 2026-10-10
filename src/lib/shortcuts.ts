import { useEffect, useState } from "react";

export type ShortcutAction =
  | "palette"
  | "newRequest"
  | "environments"
  | "import"
  | "save"
  | "sendRequest"
  | "closeTab";

export type ShortcutMap = Record<ShortcutAction, string>;

export const SHORTCUT_DEFS: { id: ShortcutAction; label: string; defaultBinding: string }[] = [
  { id: "palette", label: "Command palette", defaultBinding: "Mod+K" },
  { id: "newRequest", label: "New request", defaultBinding: "Mod+N" },
  { id: "sendRequest", label: "Send request", defaultBinding: "Mod+Enter" },
  { id: "save", label: "Save request or group", defaultBinding: "Mod+S" },
  { id: "closeTab", label: "Close active tab", defaultBinding: "Mod+W" },
  { id: "environments", label: "Switch environment", defaultBinding: "Mod+E" },
  { id: "import", label: "Import collection / cURL / OpenAPI", defaultBinding: "Mod+Shift+I" },
];

export const DEFAULT_SHORTCUTS: ShortcutMap = SHORTCUT_DEFS.reduce((acc, d) => {
  acc[d.id] = d.defaultBinding;
  return acc;
}, {} as ShortcutMap);

const STORAGE_KEY = "courion.shortcuts";
const listeners = new Set<() => void>();

export function loadShortcuts(): ShortcutMap {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_SHORTCUTS };
    const parsed = JSON.parse(raw) as Partial<ShortcutMap>;
    return { ...DEFAULT_SHORTCUTS, ...parsed };
  } catch {
    return { ...DEFAULT_SHORTCUTS };
  }
}

export function setShortcutBinding(action: ShortcutAction, binding: string): ShortcutMap {
  const next = { ...loadShortcuts(), [action]: binding };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // storage unavailable
  }
  listeners.forEach((fn) => fn());
  return next;
}

export function resetShortcuts(): ShortcutMap {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // storage unavailable
  }
  listeners.forEach((fn) => fn());
  return { ...DEFAULT_SHORTCUTS };
}

export function useShortcuts(): ShortcutMap {
  const [map, setMap] = useState<ShortcutMap>(() => loadShortcuts());
  useEffect(() => {
    const update = () => setMap(loadShortcuts());
    listeners.add(update);
    return () => {
      listeners.delete(update);
    };
  }, []);
  return map;
}

function normalizeKey(key: string): string {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();
  return key;
}

export function eventToBinding(e: {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Mod");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (parts.length === 0) return null;
  parts.push(normalizeKey(e.key));
  return parts.join("+");
}

export function matchesShortcut(e: KeyboardEvent, binding: string): boolean {
  if (!binding) return false;
  const parts = binding.split("+").map((p) => p.trim());
  const targetKey = parts[parts.length - 1];
  const wantMod = parts.includes("Mod") || parts.includes("Ctrl") || parts.includes("Cmd");
  const wantAlt = parts.includes("Alt");
  const wantShift = parts.includes("Shift");

  const hasMod = e.ctrlKey || e.metaKey;
  if (hasMod !== wantMod) return false;
  if (e.altKey !== wantAlt) return false;
  if (e.shiftKey !== wantShift) return false;

  return normalizeKey(e.key).toLowerCase() === targetKey.toLowerCase();
}

export function formatShortcut(binding: string): string {
  return binding.replace(/\bMod\b/g, "Ctrl");
}
