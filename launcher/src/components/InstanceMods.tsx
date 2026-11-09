import { ArrowUpCircle, Loader2, Plus, Puzzle, RefreshCw, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { cx } from "../lib/format";
import type { InstalledMod, Instance, ModUpdate } from "../lib/ipc";
import { busyKey, missingPresetMods, useMods } from "../stores/mods";
import { toast } from "../stores/toasts";
import { ModIcon } from "./ModIcon";
import { Button, Dialog, EmptyState } from "./ui";

const SOURCE_LABEL = { modrinth: "Modrinth", curseforge: "CurseForge" } as const;

function Toggle({
  checked,
  disabled,
  label,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative h-5 w-9 shrink-0 rounded-full transition-colors disabled:opacity-45",
        checked ? "bg-accent" : "bg-border",
      )}
    >
      <span
        className={cx(
          "absolute top-0.5 left-0.5 h-4 w-4 rounded-full transition-transform",
          checked ? "translate-x-4 bg-on-accent" : "bg-muted",
        )}
      />
    </button>
  );
}

function ModRow({
  instanceId,
  mod,
  update,
  locked,
  onRemove,
}: {
  instanceId: string;
  mod: InstalledMod;
  update: ModUpdate | undefined;
  /** The game is running, so files can't be changed. */
  locked: boolean;
  onRemove: (mod: InstalledMod) => void;
}) {
  const busy = useMods((s) => s.busy[busyKey(instanceId, mod.filename)]) ?? false;
  const { setEnabled, update: applyUpdate } = useMods.getState();
  const blocked = busy || locked;

  return (
    <li
      className={cx(
        "flex items-center gap-3 rounded-lg border border-border bg-surface px-3 py-2.5",
        !mod.enabled && "opacity-60",
      )}
    >
      <ModIcon url={mod.icon_url} size={36} />
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium">{mod.title}</div>
        <div className="truncate text-[12.5px] text-faint">
          {[
            mod.version_name || mod.filename,
            mod.source ? SOURCE_LABEL[mod.source] : "Added by hand",
            !mod.enabled && "Disabled",
          ]
            .filter(Boolean)
            .join(" · ")}
        </div>
      </div>
      {update && (
        <Button
          size="sm"
          disabled={blocked}
          title={`Update to ${update.new_version}`}
          onClick={() => void applyUpdate(instanceId, mod.filename)}
          icon={
            busy ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <ArrowUpCircle size={14} className="text-accent" />
            )
          }
        >
          Update
        </Button>
      )}
      <Toggle
        checked={mod.enabled}
        disabled={blocked}
        label={mod.enabled ? `Disable ${mod.title}` : `Enable ${mod.title}`}
        onChange={(enabled) => void setEnabled(instanceId, mod.filename, enabled)}
      />
      <Button
        size="sm"
        variant="ghost"
        disabled={blocked}
        aria-label={`Remove ${mod.title}`}
        title={`Remove ${mod.title}`}
        onClick={() => onRemove(mod)}
        icon={<Trash2 size={15} />}
      />
    </li>
  );
}

export function InstanceMods({ instance, running }: { instance: Instance; running: boolean }) {
  const navigate = useNavigate();
  const id = instance.id;
  const mods = useMods((s) => s.installed[id]);
  const updates = useMods((s) => s.updates[id]) ?? [];
  const installingPreset = useMods((s) => s.installingPreset[id]) ?? false;
  const checking = useMods((s) => s.checkingUpdates[id]) ?? false;
  const { load, checkUpdates, updateAll, installPreset, remove } = useMods.getState();
  const [removing, setRemoving] = useState<InstalledMod | null>(null);

  useEffect(() => {
    void load(id)
      // A quiet check on open, so update buttons appear without being asked for.
      .then(() => checkUpdates(id, true))
      .catch((e) => toast.error(String(e?.message ?? e)));
  }, [id, load, checkUpdates]);

  const browse = () => navigate(`/mods?instance=${id}`);
  const addButton = (
    <Button variant="primary" onClick={browse} icon={<Plus size={16} />}>
      Add mods
    </Button>
  );

  if (!mods) return null;
  const missing = missingPresetMods(instance, mods);

  const requestRemove = (mod: InstalledMod) => {
    // A mod Glint installed can be installed again in a click. A file the user put
    // there themselves can't, so that one is confirmed first.
    if (mod.source) void remove(id, mod);
    else setRemoving(mod);
  };

  return (
    <div className="flex h-full flex-col gap-3">
      {installingPreset ? (
        <div className="flex items-center gap-3 rounded-lg border border-border bg-surface px-4 py-3 text-muted">
          <Loader2 size={16} className="animate-spin" />
          Installing this preset's mods…
        </div>
      ) : (
        missing.length > 0 && (
          <div className="flex items-center justify-between gap-4 rounded-lg border border-border bg-surface px-4 py-3">
            <span className="text-muted">
              {missing.length === instance.mods.length
                ? `This instance's preset comes with ${missing.length} mods that aren't installed yet.`
                : `${missing.length} of this preset's mods ${missing.length === 1 ? "isn't" : "aren't"} installed.`}
            </span>
            <Button onClick={() => void installPreset(id)}>Install them</Button>
          </div>
        )
      )}

      {mods.length === 0 ? (
        <div className="min-h-0 flex-1">
          <EmptyState icon={<Puzzle size={32} strokeWidth={1.5} />} title="No mods yet" action={addButton}>
            Browse Modrinth and CurseForge, or drop mod files into this instance's mods folder.
          </EmptyState>
        </div>
      ) : (
        <>
          <div className="flex shrink-0 items-center gap-2">
            <span className="flex-1 text-[13px] text-muted">
              {mods.length} {mods.length === 1 ? "mod" : "mods"}
              {running && " · Stop the game to change them"}
            </span>
            {updates.length > 0 && (
              <Button disabled={running} onClick={() => void updateAll(id)}>
                Update all ({updates.length})
              </Button>
            )}
            <Button
              variant="ghost"
              disabled={checking}
              onClick={() => void checkUpdates(id)}
              icon={<RefreshCw size={15} className={cx(checking && "animate-spin")} />}
            >
              Check for updates
            </Button>
            {addButton}
          </div>
          <ul className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto pr-1">
            {mods.map((mod) => (
              <ModRow
                key={mod.filename}
                instanceId={id}
                mod={mod}
                update={updates.find((u) => u.filename === mod.filename)}
                locked={running}
                onRemove={requestRemove}
              />
            ))}
          </ul>
        </>
      )}

      {removing && (
        <Dialog
          title={`Remove ${removing.title}?`}
          onClose={() => setRemoving(null)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setRemoving(null)}>
                Keep it
              </Button>
              <Button
                className="bg-danger font-semibold text-on-accent hover:bg-danger/85"
                onClick={() => {
                  void remove(id, removing);
                  setRemoving(null);
                }}
              >
                Delete file
              </Button>
            </>
          }
        >
          <p className="text-muted">
            Glint didn't install this file and doesn't know where it came from, so it can't
            reinstall it for you. This deletes{" "}
            <span className="selectable text-text">{removing.filename}</span>.
          </p>
        </Dialog>
      )}
    </div>
  );
}
