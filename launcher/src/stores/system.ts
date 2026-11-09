import { create } from "zustand";
import { ipc, type SystemInfo } from "../lib/ipc";

interface SystemStore {
  info: SystemInfo | null;
  load: () => Promise<void>;
}

export const useSystem = create<SystemStore>((set) => ({
  info: null,
  load: async () => set({ info: await ipc.systemInfo() }),
}));
