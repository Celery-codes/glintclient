import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { cx, formatMemory } from "../lib/format";
import { ipc, toAppError, type Preset } from "../lib/ipc";
import { recommendedLoader, useGameVersions, useLoaderVersions } from "../lib/useVersions";
import { useInstances } from "../stores/instances";
import { Button, Dialog, Field, Input, Select } from "./ui";

export function NewInstanceDialog({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate();
  const create = useInstances((s) => s.create);

  const [presets, setPresets] = useState<Preset[]>([]);
  const [presetId, setPresetId] = useState("pvp");
  // Until the user types a name, it follows the chosen preset.
  const [customName, setCustomName] = useState<string | null>(null);
  const [snapshots, setSnapshots] = useState(false);
  const [mcVersion, setMcVersion] = useState<string | null>(null);
  const [loaderChoice, setLoaderChoice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const versions = useGameVersions(snapshots);
  const { loaders, error: loaderError } = useLoaderVersions(mcVersion);

  useEffect(() => {
    void ipc.listPresets().then(setPresets);
  }, []);

  // Default to the newest release once the list arrives, and again if the selected
  // version drops out of the list (a snapshot, after hiding snapshots).
  useEffect(() => {
    if (!versions.data) return;
    const list = versions.data.versions;
    if (mcVersion && list.some((v) => v.id === mcVersion)) return;
    const latest = list.find((v) => v.id === versions.data?.latest_release) ?? list[0];
    if (latest) setMcVersion(latest.id);
  }, [versions.data, mcVersion]);

  const preset = presets.find((p) => p.id === presetId) ?? null;
  const name = customName ?? preset?.name ?? "";
  const loader =
    loaders && loaderChoice && loaders.some((l) => l.version === loaderChoice)
      ? loaderChoice
      : loaders
        ? recommendedLoader(loaders)
        : null;
  const loadError = versions.error ?? loaderError;
  const ready = name.trim() !== "" && mcVersion !== null && loader !== null && !busy;

  const submit = async () => {
    if (!ready || !mcVersion || !loader) return;
    setBusy(true);
    setError(null);
    try {
      const instance = await create({
        name: name.trim(),
        preset: presetId,
        mc_version: mcVersion,
        loader_version: loader,
      });
      onClose();
      navigate(`/instances/${instance.id}`);
    } catch (e) {
      setError(toAppError(e).message);
      setBusy(false);
    }
  };

  return (
    <Dialog
      title="New instance"
      onClose={onClose}
      width="max-w-lg"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" disabled={!ready} onClick={submit}>
            {busy ? "Creating…" : "Create"}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium">Preset</span>
          <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label="Preset">
            {presets.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={p.id === presetId}
                onClick={() => setPresetId(p.id)}
                className={cx(
                  "flex flex-col gap-1 rounded-md border p-3 text-left transition-colors",
                  p.id === presetId
                    ? "border-accent bg-accent/10"
                    : "border-border hover:border-faint",
                )}
              >
                <span className="font-semibold">{p.name}</span>
                <span className="text-[12px] leading-snug text-muted">{p.description}</span>
                <span className="mt-1 text-[12px] text-faint">
                  {formatMemory(p.memory_mb)} · {p.mods.length} mods
                </span>
              </button>
            ))}
          </div>
        </div>

        <Field label="Name">
          {(id) => (
            <Input
              id={id}
              autoFocus
              value={name}
              maxLength={60}
              onChange={(e) => setCustomName(e.target.value)}
            />
          )}
        </Field>

        <div className="grid grid-cols-2 gap-3">
          <Field label="Minecraft version">
            {(id) => (
              <Select
                id={id}
                value={mcVersion ?? ""}
                disabled={!versions.data}
                onChange={(e) => {
                  setMcVersion(e.target.value);
                  setLoaderChoice(null);
                }}
              >
                {!versions.data && <option value="">Loading…</option>}
                {versions.data?.versions.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.id}
                    {v.id === versions.data?.latest_release ? " (latest)" : ""}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Fabric Loader">
            {(id) => (
              <Select
                id={id}
                value={loader ?? ""}
                disabled={!loaders}
                onChange={(e) => setLoaderChoice(e.target.value)}
              >
                {!loaders && <option value="">Loading…</option>}
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

        <label className="flex w-fit items-center gap-2 text-[13px] text-muted">
          <input
            type="checkbox"
            checked={snapshots}
            onChange={(e) => setSnapshots(e.target.checked)}
            className="accent-accent"
          />
          Show snapshots
        </label>

        {(error ?? loadError) && (
          <p className="selectable text-[13px] text-danger">{error ?? loadError}</p>
        )}
        {/* Lets Enter submit from the text field. */}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
