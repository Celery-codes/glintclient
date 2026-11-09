import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, Copy, FolderOpen, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Navigate, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { InstanceMods } from "../components/InstanceMods";
import { LogView } from "../components/LogView";
import { MemorySlider } from "../components/MemorySlider";
import { PlayButton, RunStatus } from "../components/PlayButton";
import { Button, Dialog, Field, Input, Select } from "../components/ui";
import { cx } from "../lib/format";
import { ipc, toAppError, type GcPreset, type Instance } from "../lib/ipc";
import { recommendedLoader, useGameVersions, useLoaderVersions } from "../lib/useVersions";
import { useInstances } from "../stores/instances";
import { useSystem } from "../stores/system";
import { toast } from "../stores/toasts";

const TABS = ["log", "mods", "settings"] as const;
type Tab = (typeof TABS)[number];

const GC_HINTS: Record<GcPreset, string> = {
  g1: "Tuned for short, steady pauses. The safe choice for almost every setup.",
  zgc: "Near-zero pauses at the cost of some CPU and memory. Worth trying with 8 GB or more allocated and a CPU with cores to spare. Needs Java 21 or newer.",
};

export function InstanceDetail() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const instance = useInstances((s) => s.instances.find((i) => i.id === id));
  const loaded = useInstances((s) => s.loaded);
  const run = useInstances((s) => s.runs[id]);
  const offlineLaunch = useSystem((s) => s.info?.offline_launch ?? false);
  const { play, remove, duplicate } = useInstances.getState();
  // The tab lives in the URL so other pages can link straight to one.
  const [params, setParams] = useSearchParams();
  const tab: Tab = TABS.find((t) => t === params.get("tab")) ?? "log";
  const setTab = (next: Tab) => setParams(next === "log" ? {} : { tab: next }, { replace: true });
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  if (!instance) {
    // Deleted, or a stale link.
    return loaded ? <Navigate to="/instances" replace /> : null;
  }

  const guard = (action: () => Promise<unknown>) => () =>
    void action().catch((e) => toast.error(toAppError(e).message));

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 flex-col gap-4 px-8 pt-5 pb-4">
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            aria-label="Back to instances"
            onClick={() => navigate("/instances")}
            icon={<ArrowLeft size={18} />}
          />
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-[20px] font-semibold tracking-tight">{instance.name}</h1>
            <p className="text-[13px] text-muted">
              Minecraft {instance.mc_version} · Fabric Loader {instance.loader_version}
            </p>
          </div>
          <Button
            variant="ghost"
            title="Open the instance's game folder"
            onClick={guard(() => ipc.openInstanceFolder(instance.id))}
            icon={<FolderOpen size={16} />}
          >
            Folder
          </Button>
          {offlineLaunch && !run && (
            <Button
              title="Development builds only: start without signing in"
              onClick={() => play(instance.id, true)}
            >
              Play offline
            </Button>
          )}
          <PlayButton id={instance.id} className="min-w-24" />
        </div>
        {run && (
          <div className="max-w-md">
            <RunStatus run={run} />
          </div>
        )}
        <div className="flex gap-1 border-b border-border" role="tablist">
          {TABS.map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              onClick={() => setTab(t)}
              className={cx(
                "-mb-px border-b-2 px-3 pb-2.5 font-medium capitalize transition-colors",
                tab === t
                  ? "border-accent text-text"
                  : "border-transparent text-muted hover:text-text",
              )}
            >
              {t}
            </button>
          ))}
        </div>
      </header>

      <div className="min-h-0 flex-1 px-8 pb-6">
        {tab === "log" ? (
          <LogView id={instance.id} />
        ) : tab === "mods" ? (
          <InstanceMods key={instance.id} instance={instance} running={!!run} />
        ) : (
          <div className="h-full overflow-y-auto pr-2">
            {/* Keyed so switching instances resets the form. */}
            <SettingsForm key={instance.id} instance={instance} />
            <section className="mt-8 flex max-w-xl items-center gap-2 border-t border-border pt-5">
              <Button
                disabled={!!run}
                onClick={guard(async () => {
                  const copy = await duplicate(instance.id);
                  navigate(`/instances/${copy.id}`);
                })}
                icon={<Copy size={15} />}
              >
                Duplicate
              </Button>
              <Button
                variant="danger"
                disabled={!!run}
                onClick={() => setConfirmingDelete(true)}
                icon={<Trash2 size={15} />}
              >
                Delete
              </Button>
            </section>
          </div>
        )}
      </div>

      {confirmingDelete && (
        <Dialog
          title={`Delete ${instance.name}?`}
          onClose={() => setConfirmingDelete(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirmingDelete(false)}>
                Keep it
              </Button>
              <Button
                className="bg-danger font-semibold text-on-accent hover:bg-danger/85"
                onClick={guard(async () => {
                  await remove(instance.id);
                  navigate("/instances");
                })}
              >
                Delete instance
              </Button>
            </>
          }
        >
          <p className="text-muted">
            This permanently deletes the instance's worlds, mods, screenshots and settings. It
            can't be undone.
          </p>
        </Dialog>
      )}
    </div>
  );
}

function SettingsForm({ instance }: { instance: Instance }) {
  const limits = useSystem((s) => s.info?.memory);
  const update = useInstances((s) => s.update);
  const [draft, setDraft] = useState(instance);
  const [saving, setSaving] = useState(false);

  // Snapshots stay listed if this instance already uses one.
  const versions = useGameVersions(/-|w/.test(instance.mc_version));
  const { loaders } = useLoaderVersions(draft.mc_version);

  // A different Minecraft version has its own loader list: move to its recommended
  // loader if the current one isn't available there.
  useEffect(() => {
    if (!loaders || loaders.some((l) => l.version === draft.loader_version)) return;
    const loader = recommendedLoader(loaders);
    if (loader) setDraft((d) => ({ ...d, loader_version: loader }));
  }, [loaders, draft.loader_version]);

  const set = <K extends keyof Instance>(key: K, value: Instance[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(instance);
  const valid = draft.name.trim() !== "";

  const save = async () => {
    setSaving(true);
    try {
      setDraft(await update({ ...draft, name: draft.name.trim() }));
      toast.success("Settings saved");
    } catch (e) {
      toast.error(toAppError(e).message);
    } finally {
      setSaving(false);
    }
  };

  const browseJava = async () => {
    const picked = await openFileDialog({
      title: "Choose a Java executable",
      multiple: false,
      filters: [{ name: "Java", extensions: ["exe"] }],
    });
    if (typeof picked === "string") set("java_path", picked);
  };

  const dimension = (key: "width" | "height") => (
    <Input
      inputMode="numeric"
      placeholder={key === "width" ? "Width" : "Height"}
      aria-label={key === "width" ? "Window width" : "Window height"}
      value={draft[key] ?? ""}
      onChange={(e) => {
        const n = Number.parseInt(e.target.value.replace(/\D/g, ""), 10);
        set(key, Number.isNaN(n) ? null : Math.min(n, 16384));
      }}
    />
  );

  return (
    <form
      className="flex max-w-xl flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault();
        if (dirty && valid) void save();
      }}
    >
      <Field label="Name">
        {(id) => (
          <Input id={id} value={draft.name} maxLength={60} onChange={(e) => set("name", e.target.value)} />
        )}
      </Field>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Minecraft version">
          {(id) => (
            <Select id={id} value={draft.mc_version} onChange={(e) => set("mc_version", e.target.value)}>
              {/* Keeps the current value selectable while the list loads. */}
              {!versions.data?.versions.some((v) => v.id === draft.mc_version) && (
                <option value={draft.mc_version}>{draft.mc_version}</option>
              )}
              {versions.data?.versions.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.id}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Fabric Loader">
          {(id) => (
            <Select
              id={id}
              value={draft.loader_version}
              onChange={(e) => set("loader_version", e.target.value)}
            >
              {!loaders?.some((l) => l.version === draft.loader_version) && (
                <option value={draft.loader_version}>{draft.loader_version}</option>
              )}
              {loaders?.map((l) => (
                <option key={l.version} value={l.version}>
                  {l.version}
                  {l.stable ? "" : " (unstable)"}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>

      {limits && (
        <Field label="Memory">
          {(id) => (
            <MemorySlider
              id={id}
              value={draft.memory_mb}
              limits={limits}
              onChange={(mb) => set("memory_mb", mb)}
            />
          )}
        </Field>
      )}

      <Field label="Garbage collector" hint={GC_HINTS[draft.gc]}>
        {(id) => (
          <Select id={id} value={draft.gc} onChange={(e) => set("gc", e.target.value as GcPreset)}>
            <option value="g1">G1 (recommended)</option>
            <option value="zgc">ZGC</option>
          </Select>
        )}
      </Field>

      <Field
        label="Extra JVM arguments"
        hint="Added after Glint's own flags, so anything here overrides them. Leave empty unless you know you need it."
      >
        {(id) => (
          <Input
            id={id}
            className="font-mono text-[13px]"
            placeholder="-Dexample=true"
            value={draft.jvm_args}
            onChange={(e) => set("jvm_args", e.target.value)}
          />
        )}
      </Field>

      <Field
        label="Java"
        hint="Leave empty and Glint downloads the Java version this Minecraft version needs."
      >
        {(id) => (
          <div className="flex gap-2">
            <Input
              id={id}
              placeholder="Managed by Glint"
              value={draft.java_path ?? ""}
              onChange={(e) => set("java_path", e.target.value || null)}
            />
            <Button onClick={() => void browseJava()}>Browse</Button>
          </div>
        )}
      </Field>

      <Field label="Window size" hint="Leave empty to use the game's default.">
        {() => (
          <div className="flex max-w-60 items-center gap-2">
            {dimension("width")}
            <span className="text-faint">×</span>
            {dimension("height")}
          </div>
        )}
      </Field>

      <div className="flex items-center gap-2">
        <Button type="submit" variant="primary" disabled={!dirty || !valid || saving}>
          {saving ? "Saving…" : "Save changes"}
        </Button>
        {dirty && (
          <Button variant="ghost" onClick={() => setDraft(instance)}>
            Discard
          </Button>
        )}
      </div>
    </form>
  );
}
