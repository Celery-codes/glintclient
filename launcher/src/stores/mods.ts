import { openUrl } from "@tauri-apps/plugin-opener";
import { create } from "zustand";
import {
  ipc,
  toAppError,
  type InstallOutcome,
  type InstalledMod,
  type Instance,
  type ModSource,
  type ModUpdate,
} from "../lib/ipc";
import { toast } from "./toasts";

interface ModStore {
  /** Installed mods per instance; absent until first loaded. */
  installed: Record<string, InstalledMod[]>;
  updates: Record<string, ModUpdate[]>;
  /** Instances whose preset mods are being installed. */
  installingPreset: Record<string, boolean>;
  checkingUpdates: Record<string, boolean>;
  /** Keys of operations in flight, for per-row spinners. */
  busy: Record<string, boolean>;

  load: (id: string) => Promise<void>;
  install: (id: string, source: ModSource, project: string, title: string) => Promise<void>;
  installPreset: (id: string) => Promise<void>;
  setEnabled: (id: string, filename: string, enabled: boolean) => Promise<void>;
  remove: (id: string, mod: InstalledMod) => Promise<void>;
  checkUpdates: (id: string, quiet?: boolean) => Promise<void>;
  update: (id: string, filename: string) => Promise<void>;
  updateAll: (id: string) => Promise<void>;
}

export const busyKey = (id: string, what: string) => `${id}:${what}`;

/** Whether an installed mod is this project on this site (by id or slug). */
export function isProject(mod: InstalledMod, source: string, project: string): boolean {
  return (
    mod.source === source &&
    (mod.project_id === project || mod.slug?.toLowerCase() === project.toLowerCase())
  );
}

/** Preset mods that aren't in the instance yet. */
export function missingPresetMods(instance: Instance, installed: InstalledMod[]) {
  return instance.mods.filter((ref) => !installed.some((m) => isProject(m, ref.source, ref.project)));
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

/** Tell the user what an install did, and open pages for mods that need a manual download. */
function report(outcome: InstallOutcome, requested: string) {
  const { installed, already_installed, manual, failed } = outcome;
  if (installed.length === 1) {
    toast.success(`Installed ${installed[0].title}`);
  } else if (installed.length > 1) {
    const main = installed.find((m) => m.title === requested) ?? installed[0];
    toast.success(`Installed ${main.title} and ${plural(installed.length - 1, "other mod")}`);
  } else if (already_installed.length > 0 && manual.length === 0 && failed.length === 0) {
    toast.info(`${already_installed[0]} is already installed`);
  }
  for (const item of manual) {
    toast.info(
      `${item.title} only allows downloads from its own page, which is now open in your browser. Save the file into this instance's mods folder.`,
    );
    void openUrl(item.page_url);
  }
  for (const failure of failed) {
    toast.error(`Couldn't install ${failure.project}. ${failure.message}`);
  }
}

export const useMods = create<ModStore>((set, get) => {
  const setBusy = (key: string, value: boolean) => {
    const { [key]: _old, ...rest } = get().busy;
    set({ busy: value ? { ...rest, [key]: true } : rest });
  };
  const withBusy = async (key: string, action: () => Promise<void>) => {
    if (get().busy[key]) return;
    setBusy(key, true);
    try {
      await action();
    } catch (error) {
      toast.error(toAppError(error).message);
    } finally {
      setBusy(key, false);
    }
  };
  const dropUpdate = (id: string, filename: string) =>
    set({
      updates: {
        ...get().updates,
        [id]: (get().updates[id] ?? []).filter((u) => u.filename !== filename),
      },
    });

  return {
    installed: {},
    updates: {},
    installingPreset: {},
    checkingUpdates: {},
    busy: {},

    load: async (id) => {
      const mods = await ipc.listInstanceMods(id);
      set({ installed: { ...get().installed, [id]: mods } });
    },

    install: (id, source, project, title) =>
      withBusy(busyKey(id, `${source}/${project}`), async () => {
        const outcome = await ipc.installMod(id, source, project);
        report(outcome, title);
        await get().load(id);
      }),

    installPreset: async (id) => {
      if (get().installingPreset[id]) return;
      set({ installingPreset: { ...get().installingPreset, [id]: true } });
      try {
        const outcome = await ipc.installPresetMods(id);
        if (outcome.installed.length > 0) {
          toast.success(`Installed ${plural(outcome.installed.length, "mod")}`);
        }
        report({ ...outcome, installed: [], already_installed: [] }, "");
        await get().load(id);
      } catch (error) {
        toast.error(`Couldn't install the preset's mods. ${toAppError(error).message}`);
      } finally {
        set({ installingPreset: { ...get().installingPreset, [id]: false } });
      }
    },

    setEnabled: (id, filename, enabled) =>
      withBusy(busyKey(id, filename), async () => {
        const updated = await ipc.setModEnabled(id, filename, enabled);
        set({
          installed: {
            ...get().installed,
            [id]: (get().installed[id] ?? []).map((m) => (m.filename === filename ? updated : m)),
          },
        });
      }),

    remove: (id, mod) =>
      withBusy(busyKey(id, mod.filename), async () => {
        await ipc.removeMod(id, mod.filename);
        dropUpdate(id, mod.filename);
        toast.info(`Removed ${mod.title}`);
        await get().load(id);
      }),

    checkUpdates: async (id, quiet = false) => {
      if (get().checkingUpdates[id]) return;
      set({ checkingUpdates: { ...get().checkingUpdates, [id]: true } });
      try {
        const updates = await ipc.checkModUpdates(id);
        set({ updates: { ...get().updates, [id]: updates } });
        // Checking can also recognise hand-copied jars, which changes the list.
        await get().load(id);
        if (!quiet) {
          toast.info(
            updates.length > 0
              ? `${plural(updates.length, "update")} available`
              : "All mods are up to date",
          );
        }
      } catch (error) {
        if (!quiet) toast.error(`Couldn't check for updates. ${toAppError(error).message}`);
      } finally {
        set({ checkingUpdates: { ...get().checkingUpdates, [id]: false } });
      }
    },

    update: (id, filename) =>
      withBusy(busyKey(id, filename), async () => {
        const updated = await ipc.updateMod(id, filename);
        dropUpdate(id, filename);
        toast.success(`Updated ${updated.title} to ${updated.version_name}`);
        await get().load(id);
      }),

    updateAll: async (id) => {
      for (const update of get().updates[id] ?? []) {
        await get().update(id, update.filename);
      }
    },
  };
});
