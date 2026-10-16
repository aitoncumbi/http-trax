import { create } from "zustand";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { AgentClient, AgentOverview, AgentPermissions } from "@/lib/types";

interface AgentsState {
  overview: AgentOverview | null;
  /** Client whose setup snippet is shown. */
  client: AgentClient["id"];
  setClient: (id: AgentClient["id"]) => void;
  load: (workspaceId: string | null) => Promise<void>;
  setPermission: (key: keyof AgentPermissions, on: boolean) => Promise<void>;
  setMcpPort: (port: number | null) => Promise<void>;
}

/** What the AI agents view shows; shared by its sidebar list and main card. */
export const useAgents = create<AgentsState>((set, get) => ({
  overview: null,
  client: "code",
  setClient: (client) => set({ client }),
  load: async (workspaceId) => {
    try {
      set({ overview: await api.agentOverview(workspaceId) });
    } catch (e) {
      useStore.getState().toast("error", errorMessage(e));
    }
  },
  setMcpPort: async (port) => {
    const current = get().overview;
    if (!current) return;
    set({ overview: { ...current, mcpPort: port ?? undefined } });
    try {
      await api.setMcpPort(port);
      await get().load(null);
    } catch (e) {
      set({ overview: current });
      useStore.getState().toast("error", errorMessage(e));
    }
  },
  setPermission: async (key, on) => {
    const current = get().overview;
    if (!current) return;
    const permissions = { ...current.permissions, [key]: on };
    set({ overview: { ...current, permissions } });
    try {
      await api.setAgentPermissions(permissions);
    } catch (e) {
      set({ overview: current });
      useStore.getState().toast("error", errorMessage(e));
    }
  },
}));

export const CLIENT_MONO: Record<AgentClient["id"], string> = { code: "CC", desktop: "CD", vscode: "VS", other: "AI" };

export function statusLabel(c: AgentClient): string {
  switch (c.status) {
    case "connected":
      return "Connected";
    case "configured":
      return "Set up";
    default:
      return "Not set up";
  }
}
