import { create } from "zustand";
import {
  ipc,
  toAppError,
  type AppError,
  type Instance,
  type InstanceProgressEvent,
  type InstanceStateEvent,
  type NewInstance,
  type RunState,
} from "../lib/ipc";
import { useAccounts } from "./accounts";
import { clearLog } from "./logs";
import { toast } from "./toasts";

export interface Run {
  state: RunState;
  progress: { stage: string; done: number; total: number } | null;
}

/** How the most recent run of an instance ended, when that's worth showing. */
export interface Exit {
  code: number | null;
  error: AppError | null;
}

interface InstanceStore {
  instances: Instance[];
  loaded: boolean;
  runs: Record<string, Run>;
  exits: Record<string, Exit>;
  load: () => Promise<void>;
  create: (instance: NewInstance) => Promise<Instance>;
  update: (instance: Instance) => Promise<Instance>;
  remove: (id: string) => Promise<void>;
  duplicate: (id: string) => Promise<Instance>;
  /** Start the game, signing in first if nobody is. */
  play: (id: string, offline?: boolean) => void;
  stop: (id: string) => void;
  onState: (event: InstanceStateEvent) => void;
  onProgress: (event: InstanceProgressEvent) => void;
}

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const { [key]: _removed, ...rest } = record;
  return rest;
}

export const useInstances = create<InstanceStore>((set, get) => ({
  instances: [],
  loaded: false,
  runs: {},
  exits: {},

  load: async () => {
    const [instances, states] = await Promise.all([ipc.listInstances(), ipc.instanceStates()]);
    const runs: Record<string, Run> = {};
    for (const [id, state] of Object.entries(states)) {
      runs[id] = get().runs[id] ?? { state, progress: null };
    }
    set({ instances, runs, loaded: true });
  },

  create: async (instance) => {
    const created = await ipc.createInstance(instance);
    await get().load();
    return created;
  },

  update: async (instance) => {
    const updated = await ipc.updateInstance(instance);
    set({ instances: get().instances.map((i) => (i.id === updated.id ? updated : i)) });
    return updated;
  },

  remove: async (id) => {
    await ipc.deleteInstance(id);
    clearLog(id);
    set({
      instances: get().instances.filter((i) => i.id !== id),
      exits: without(get().exits, id),
    });
  },

  duplicate: async (id) => {
    const copy = await ipc.duplicateInstance(id);
    await get().load();
    return copy;
  },

  play: (id, offline = false) => {
    if (get().runs[id]) return;
    if (!offline && !useAccounts.getState().active()) {
      void useAccounts.getState().beginLogin(() => get().play(id));
      return;
    }
    // Show "preparing" at once; the backend confirms with an event.
    set({
      runs: { ...get().runs, [id]: { state: "preparing", progress: null } },
      exits: without(get().exits, id),
    });
    ipc.launchInstance(id, offline).catch((error) => {
      set({ runs: without(get().runs, id) });
      toast.error(toAppError(error).message);
    });
  },

  stop: (id) => void ipc.stopInstance(id),

  onState: (event) => {
    const { id } = event;
    if (event.state !== "stopped") {
      if (event.state === "preparing") clearLog(id);
      const progress = event.state === "preparing" ? (get().runs[id]?.progress ?? null) : null;
      set({ runs: { ...get().runs, [id]: { state: event.state, progress } } });
      return;
    }

    const name = get().instances.find((i) => i.id === id)?.name ?? "The game";
    const crashed = !event.killed && event.exit_code !== null && event.exit_code !== 0;
    set({ runs: without(get().runs, id) });
    if (event.error) {
      set({ exits: { ...get().exits, [id]: { code: null, error: event.error } } });
      // An expired or missing sign-in is fixed by signing in, so go straight there.
      const kind = event.error.kind;
      if (kind === "auth_not_signed_in" || kind === "auth_session_expired") {
        void useAccounts.getState().beginLogin(() => get().play(id));
      } else {
        toast.error(`${name} couldn't start. ${event.error.message}`);
      }
    } else if (crashed) {
      set({ exits: { ...get().exits, [id]: { code: event.exit_code, error: null } } });
      toast.error(`${name} exited unexpectedly (code ${event.exit_code}). Check its log.`);
    }
    // Picks up the new "last played" time and ordering.
    void get().load();
  },

  onProgress: (event) => {
    const run = get().runs[event.id];
    if (run?.state !== "preparing") return;
    const { stage, done, total } = event;
    set({ runs: { ...get().runs, [event.id]: { ...run, progress: { stage, done, total } } } });
  },
}));
