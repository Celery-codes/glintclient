import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FolderOpen } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Button, Field, Input, Page } from "../components/ui";
import { ipc, toAppError, type SettingSource, type Settings as SettingsData } from "../lib/ipc";
import { useSystem } from "../stores/system";
import { toast } from "../stores/toasts";

const REVIEW_FORM = "https://aka.ms/mce-reviewappid";
const AZURE_PORTAL =
  "https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-4 border-b border-border py-6 first:pt-1 last:border-0">
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {children}
    </section>
  );
}

function ExternalLinkButton({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={() => void openUrl(href)}
      className="inline-flex items-center gap-1 text-accent hover:text-accent-hover hover:underline"
    >
      {children}
      <ExternalLink size={12} />
    </button>
  );
}

/** Explains a value that is in effect but doesn't come from this form. */
function sourceNote(source: SettingSource | null, envVar: string): string | null {
  if (source === "env") return `Currently overridden by the ${envVar} environment variable.`;
  if (source === "builtin") return "Leave empty to use the ID built into this copy of Glint.";
  return null;
}

export function Settings() {
  const info = useSystem((s) => s.info);
  const [saved, setSaved] = useState<SettingsData | null>(null);
  const [clientId, setClientId] = useState("");
  const [curseforgeKey, setCurseforgeKey] = useState("");
  const [downloads, setDownloads] = useState(16);
  const [saving, setSaving] = useState(false);

  const apply = (settings: SettingsData) => {
    setSaved(settings);
    setClientId(settings.azure_client_id ?? "");
    setCurseforgeKey(settings.curseforge_api_key ?? "");
    setDownloads(settings.max_concurrent_downloads);
  };

  useEffect(() => {
    ipc
      .getSettings()
      .then(apply)
      .catch((e) => toast.error(toAppError(e).message));
  }, []);

  if (!saved) return <Page title="Settings">{null}</Page>;

  const dirty =
    clientId.trim() !== (saved.azure_client_id ?? "") ||
    curseforgeKey.trim() !== (saved.curseforge_api_key ?? "") ||
    downloads !== saved.max_concurrent_downloads;

  const save = async () => {
    setSaving(true);
    try {
      apply(
        await ipc.saveSettings({
          azure_client_id: clientId.trim() || null,
          curseforge_api_key: curseforgeKey.trim() || null,
          max_concurrent_downloads: downloads,
        }),
      );
      toast.success("Settings saved");
    } catch (e) {
      toast.error(toAppError(e).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Page
      title="Settings"
      actions={
        <Button variant="primary" disabled={!dirty || saving} onClick={() => void save()}>
          {saving ? "Saving…" : "Save changes"}
        </Button>
      }
    >
      <div className="max-w-xl">
        <Section title="Microsoft sign-in">
          <Field
            label="Azure application (client) ID"
            hint={
              sourceNote(saved.azure_client_id_source, "GLINT_AZURE_CLIENT_ID") ??
              "Identifies Glint to Microsoft when you sign in. It isn't a secret."
            }
          >
            {(id) => (
              <Input
                id={id}
                className="font-mono text-[13px]"
                placeholder="00000000-0000-0000-0000-000000000000"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
              />
            )}
          </Field>
          <ol className="flex list-decimal flex-col gap-1.5 pl-5 text-[13px] leading-snug text-muted">
            <li>
              Register an app in the{" "}
              <ExternalLinkButton href={AZURE_PORTAL}>Azure portal</ExternalLinkButton> for{" "}
              <span className="text-text">personal Microsoft accounts only</span>, with no
              redirect URI.
            </li>
            <li>
              Under Authentication, turn on{" "}
              <span className="text-text">Allow public client flows</span>.
            </li>
            <li>Paste its Application (client) ID above.</li>
            <li>
              Ask Mojang to approve the app with the{" "}
              <ExternalLinkButton href={REVIEW_FORM}>review form</ExternalLinkButton>. Until
              they do, sign-in stops at the last step.
            </li>
          </ol>
        </Section>

        <Section title="CurseForge">
          <Field
            label="API key"
            hint={
              sourceNote(saved.curseforge_api_key_source, "GLINT_CURSEFORGE_API_KEY") ??
              "Needed to search and install mods from CurseForge. Modrinth needs no key."
            }
          >
            {(id) => (
              <Input
                id={id}
                type="password"
                className="font-mono text-[13px]"
                value={curseforgeKey}
                onChange={(e) => setCurseforgeKey(e.target.value)}
              />
            )}
          </Field>
        </Section>

        <Section title="Downloads">
          <Field
            label="Simultaneous downloads"
            hint="Lower this if installs fail or stall on a slow or unreliable connection."
          >
            {(id) => (
              <Input
                id={id}
                type="number"
                min={1}
                max={64}
                className="max-w-28"
                value={downloads}
                onChange={(e) => {
                  const n = Number.parseInt(e.target.value, 10);
                  setDownloads(Number.isNaN(n) ? 1 : Math.min(64, Math.max(1, n)));
                }}
              />
            )}
          </Field>
        </Section>

        {info && (
          <Section title="About">
            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-[13px]">
              <dt className="text-muted">Version</dt>
              <dd className="selectable">Glint {info.launcher_version}</dd>
              <dt className="text-muted">Data folder</dt>
              <dd className="selectable min-w-0 break-all">{info.data_dir}</dd>
            </dl>
            <div>
              <Button
                size="sm"
                icon={<FolderOpen size={14} />}
                onClick={() =>
                  void ipc.openDataFolder().catch((e) => toast.error(toAppError(e).message))
                }
              >
                Open data folder
              </Button>
            </div>
          </Section>
        )}
      </div>
    </Page>
  );
}
