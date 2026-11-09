// Typed wrappers around the Rust commands. Field names match the Rust structs
// (snake_case); command arguments are camelCase, which Tauri maps to snake_case.
import { invoke } from "@tauri-apps/api/core";

export type GcPreset = "g1" | "zgc";

export interface ModRef {
  source: string;
  project: string;
}

export interface Instance {
  id: string;
  name: string;
  preset: string | null;
  mc_version: string;
  loader_version: string;
  memory_mb: number;
  gc: GcPreset;
  jvm_args: string;
  java_path: string | null;
  width: number | null;
  height: number | null;
  mods: ModRef[];
  created_at: number;
  last_played: number | null;
}

export interface NewInstance {
  name: string;
  preset: string | null;
  mc_version: string;
  loader_version: string;
}

export interface Preset {
  id: string;
  name: string;
  description: string;
  memory_mb: number;
  gc: GcPreset;
  mods: ModRef[];
}

export interface GameVersion {
  id: string;
  kind: string;
  release_time: string;
}

export interface GameVersions {
  latest_release: string;
  versions: GameVersion[];
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export interface MemoryLimits {
  total_mb: number;
  min_mb: number;
  max_recommended_mb: number;
  diminishing_mb: number;
  slider_max_mb: number;
}

export interface SystemInfo {
  launcher_version: string;
  memory: MemoryLimits;
  data_dir: string;
  offline_launch: boolean;
}

export type SettingSource = "env" | "file" | "builtin";

export interface Settings {
  azure_client_id: string | null;
  azure_client_id_source: SettingSource | null;
  curseforge_api_key: string | null;
  curseforge_api_key_source: SettingSource | null;
  max_concurrent_downloads: number;
}

export interface SettingsInput {
  azure_client_id: string | null;
  curseforge_api_key: string | null;
  max_concurrent_downloads: number;
}

export interface Account {
  uuid: string;
  username: string;
  added_at: number;
}

export interface Accounts {
  active: string | null;
  accounts: Account[];
}

export interface LoginPrompt {
  user_code: string;
  verification_uri: string;
  expires_in: number;
}

export interface LogLine {
  text: string;
  err: boolean;
}

/** Every command rejects with this shape. `kind` is stable; `message` is for people. */
export interface AppError {
  kind: string;
  message: string;
}

export function toAppError(error: unknown): AppError {
  if (typeof error === "object" && error !== null && "message" in error) {
    const e = error as Partial<AppError>;
    return { kind: e.kind ?? "other", message: String(e.message) };
  }
  return { kind: "other", message: String(error) };
}

export type RunState = "preparing" | "running";

export interface InstanceStateEvent {
  id: string;
  state: RunState | "stopped";
  exit_code: number | null;
  killed: boolean;
  error: AppError | null;
}

export interface InstanceProgressEvent {
  id: string;
  stage: string;
  done: number;
  total: number;
}

export interface InstanceLogEvent {
  id: string;
  lines: LogLine[];
}

export interface AuthFinishedEvent {
  account: Account | null;
  error: AppError | null;
}

export type ModSource = "modrinth" | "curseforge";

export interface ModCategory {
  id: string;
  label: string;
}

export interface SearchHit {
  source: ModSource;
  project_id: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  icon_url: string | null;
  downloads: number;
  page_url: string;
  /** False when the author only allows downloads from the mod's own page. */
  downloadable: boolean;
}

export interface SearchRequest {
  text: string;
  source: "all" | ModSource;
  category: string | null;
  mc_version: string | null;
  fabric_only: boolean;
  page: number;
}

export interface SearchResponse {
  hits: SearchHit[];
  has_more: boolean;
  curseforge_enabled: boolean;
  warnings: string[];
}

export interface InstalledMod {
  filename: string;
  enabled: boolean;
  title: string;
  version_name: string;
  source: ModSource | null;
  project_id: string | null;
  slug: string | null;
  version_id: string | null;
  icon_url: string | null;
  page_url: string | null;
  size: number;
}

export interface InstallOutcome {
  installed: InstalledMod[];
  already_installed: string[];
  manual: { title: string; page_url: string }[];
  failed: { project: string; message: string }[];
}

export interface ModUpdate {
  filename: string;
  title: string;
  current_version: string;
  new_version: string;
}

export const ipc = {
  systemInfo: () => invoke<SystemInfo>("system_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: SettingsInput) => invoke<Settings>("save_settings", { settings }),
  openDataFolder: () => invoke<void>("open_data_folder"),

  listGameVersions: (includeSnapshots: boolean) =>
    invoke<GameVersions>("list_game_versions", { includeSnapshots }),
  listLoaderVersions: (mcVersion: string) =>
    invoke<LoaderVersion[]>("list_loader_versions", { mcVersion }),

  listPresets: () => invoke<Preset[]>("list_presets"),
  listInstances: () => invoke<Instance[]>("list_instances"),
  createInstance: (instance: NewInstance) => invoke<Instance>("create_instance", { new: instance }),
  updateInstance: (instance: Instance) => invoke<Instance>("update_instance", { instance }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  duplicateInstance: (id: string) => invoke<Instance>("duplicate_instance", { id }),
  openInstanceFolder: (id: string) => invoke<void>("open_instance_folder", { id }),

  modCategories: () => invoke<ModCategory[]>("mod_categories"),
  searchMods: (request: SearchRequest) => invoke<SearchResponse>("search_mods", { request }),
  listInstanceMods: (id: string) => invoke<InstalledMod[]>("list_instance_mods", { id }),
  installMod: (id: string, source: ModSource, project: string) =>
    invoke<InstallOutcome>("install_mod", { id, source, project }),
  installPresetMods: (id: string) => invoke<InstallOutcome>("install_preset_mods", { id }),
  setModEnabled: (id: string, filename: string, enabled: boolean) =>
    invoke<InstalledMod>("set_mod_enabled", { id, filename, enabled }),
  removeMod: (id: string, filename: string) => invoke<void>("remove_mod", { id, filename }),
  checkModUpdates: (id: string) => invoke<ModUpdate[]>("check_mod_updates", { id }),
  updateMod: (id: string, filename: string) =>
    invoke<InstalledMod>("update_mod", { id, filename }),

  launchInstance: (id: string, offline: boolean) =>
    invoke<void>("launch_instance", { id, offline }),
  stopInstance: (id: string) => invoke<void>("stop_instance", { id }),
  instanceStates: () => invoke<Record<string, RunState>>("instance_states"),
  instanceLog: (id: string) => invoke<LogLine[]>("instance_log", { id }),

  authBegin: () => invoke<LoginPrompt>("auth_begin"),
  authCancel: () => invoke<void>("auth_cancel"),
  listAccounts: () => invoke<Accounts>("list_accounts"),
  setActiveAccount: (uuid: string) => invoke<Accounts>("set_active_account", { uuid }),
  removeAccount: (uuid: string) => invoke<Accounts>("remove_account", { uuid }),
};
