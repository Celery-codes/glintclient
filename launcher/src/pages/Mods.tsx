import { openUrl } from "@tauri-apps/plugin-opener";
import { Boxes, Check, Download, ExternalLink, Loader2, Search, SearchX } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { ModIcon } from "../components/ModIcon";
import { Button, EmptyState, Input, Page, Select } from "../components/ui";
import { formatCount } from "../lib/format";
import {
  ipc,
  toAppError,
  type InstalledMod,
  type Instance,
  type ModCategory,
  type SearchHit,
  type SearchRequest,
} from "../lib/ipc";
import { useInstances } from "../stores/instances";
import { busyKey, isProject, useMods } from "../stores/mods";

const SOURCE_LABEL = { modrinth: "Modrinth", curseforge: "CurseForge" } as const;

interface Results {
  hits: SearchHit[];
  hasMore: boolean;
  curseforgeEnabled: boolean;
  warnings: string[];
}

function ResultRow({
  hit,
  instance,
  installed,
}: {
  hit: SearchHit;
  instance: Instance;
  installed: InstalledMod[];
}) {
  const busy = useMods((s) => s.busy[busyKey(instance.id, `${hit.source}/${hit.project_id}`)]);
  const install = useMods((s) => s.install);
  const isInstalled = installed.some((m) => isProject(m, hit.source, hit.project_id));

  let action;
  if (isInstalled) {
    action = (
      <Button size="sm" variant="ghost" disabled icon={<Check size={14} />}>
        Installed
      </Button>
    );
  } else if (!hit.downloadable) {
    // The author only allows downloads from the mod's page, so send the user there.
    action = (
      <Button
        size="sm"
        title="This mod's author only allows downloads from its CurseForge page"
        onClick={() => void openUrl(hit.page_url)}
        icon={<ExternalLink size={14} />}
      >
        Open page
      </Button>
    );
  } else {
    action = (
      <Button
        size="sm"
        variant="primary"
        disabled={busy}
        onClick={() => void install(instance.id, hit.source, hit.project_id, hit.title)}
        icon={busy ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
      >
        {busy ? "Installing" : "Install"}
      </Button>
    );
  }

  return (
    <li className="flex items-center gap-4 rounded-lg border border-border bg-surface px-4 py-3">
      <ModIcon url={hit.icon_url} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <button
            type="button"
            title="Open this mod's page"
            onClick={() => void openUrl(hit.page_url)}
            className="truncate font-semibold hover:text-accent"
          >
            {hit.title}
          </button>
          {hit.author && <span className="shrink-0 text-[12.5px] text-faint">by {hit.author}</span>}
        </div>
        <p className="line-clamp-2 text-[13px] leading-snug text-muted">{hit.description}</p>
      </div>
      <div className="flex w-24 shrink-0 flex-col items-end text-[12.5px] text-faint">
        <span>{SOURCE_LABEL[hit.source]}</span>
        <span className="tabular-nums">{formatCount(hit.downloads)} downloads</span>
      </div>
      <div className="flex w-28 shrink-0 justify-end">{action}</div>
    </li>
  );
}

export function Mods() {
  const instances = useInstances((s) => s.instances);
  const instancesLoaded = useInstances((s) => s.loaded);
  const [params, setParams] = useSearchParams();
  const instance = instances.find((i) => i.id === params.get("instance")) ?? instances[0];

  const installed = useMods((s) => (instance ? s.installed[instance.id] : undefined));
  const loadInstalled = useMods((s) => s.load);

  const [categories, setCategories] = useState<ModCategory[]>([]);
  const [text, setText] = useState("");
  const [debouncedText, setDebouncedText] = useState("");
  const [source, setSource] = useState<SearchRequest["source"]>("all");
  const [category, setCategory] = useState("");
  const [matchVersion, setMatchVersion] = useState(true);
  const [fabricOnly, setFabricOnly] = useState(true);

  const [results, setResults] = useState<Results | null>(null);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Guards against a slow response for an old query overwriting a newer one.
  const requestId = useRef(0);

  useEffect(() => {
    void ipc.modCategories().then(setCategories);
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedText(text), 300);
    return () => clearTimeout(timer);
  }, [text]);

  const instanceId = instance?.id;
  const mcVersion = instance?.mc_version ?? null;
  useEffect(() => {
    if (instanceId) void loadInstalled(instanceId).catch(() => {});
  }, [instanceId, loadInstalled]);

  const search = async (pageToLoad: number) => {
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const response = await ipc.searchMods({
        text: debouncedText,
        source,
        category: category || null,
        mc_version: matchVersion ? mcVersion : null,
        fabric_only: fabricOnly,
        page: pageToLoad,
      });
      if (id !== requestId.current) return;
      setPage(pageToLoad);
      setResults((previous) => ({
        hits: pageToLoad === 0 ? response.hits : [...(previous?.hits ?? []), ...response.hits],
        hasMore: response.has_more,
        curseforgeEnabled: response.curseforge_enabled,
        warnings: response.warnings,
      }));
    } catch (e) {
      if (id !== requestId.current) return;
      setError(toAppError(e).message);
      if (pageToLoad === 0) setResults(null);
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  };

  // Any change to the query starts again from the first page.
  useEffect(() => {
    if (mcVersion) void search(0);
    // `search` reads the same values listed here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debouncedText, source, category, matchVersion, fabricOnly, mcVersion]);

  if (instancesLoaded && !instance) {
    return (
      <Page title="Mods">
        <EmptyState
          icon={<Boxes size={36} strokeWidth={1.5} />}
          title="Create an instance first"
          action={
            <Link to="/instances">
              <span className="text-accent hover:underline">Go to Instances</span>
            </Link>
          }
        >
          Mods are installed into an instance, so the browser needs one to install to.
        </EmptyState>
      </Page>
    );
  }
  if (!instance) return <Page title="Mods">{null}</Page>;

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 flex-col gap-3 px-8 pt-4 pb-4">
        <div className="flex h-9 items-center justify-between gap-4">
          <h1 className="text-[20px] font-semibold tracking-tight">Mods</h1>
          <label className="flex items-center gap-2 text-[13px] text-muted">
            Install to
            <Select
              className="w-56"
              value={instance.id}
              onChange={(e) => setParams({ instance: e.target.value }, { replace: true })}
            >
              {instances.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name} ({i.mc_version})
                </option>
              ))}
            </Select>
          </label>
        </div>
        <div className="flex gap-2">
          <div className="relative min-w-0 flex-1">
            <Search
              size={16}
              className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-faint"
            />
            <Input
              autoFocus
              className="pl-9"
              placeholder="Search mods"
              aria-label="Search mods"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </div>
          <Select
            className="w-36"
            aria-label="Site"
            value={source}
            onChange={(e) => setSource(e.target.value as SearchRequest["source"])}
          >
            <option value="all">All sites</option>
            <option value="modrinth">Modrinth</option>
            <option value="curseforge">CurseForge</option>
          </Select>
          <Select
            className="w-44"
            aria-label="Category"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
          >
            <option value="">All categories</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </Select>
          <Select
            className="w-36"
            aria-label="Minecraft version"
            value={matchVersion ? "match" : "any"}
            onChange={(e) => setMatchVersion(e.target.value === "match")}
          >
            <option value="match">{instance.mc_version}</option>
            <option value="any">Any version</option>
          </Select>
          <Select
            className="w-32"
            aria-label="Mod loader"
            value={fabricOnly ? "fabric" : "any"}
            onChange={(e) => setFabricOnly(e.target.value === "fabric")}
          >
            <option value="fabric">Fabric</option>
            <option value="any">Any loader</option>
          </Select>
        </div>
        {results && !results.curseforgeEnabled && source !== "modrinth" && (
          <p className="text-[12.5px] text-faint">
            Showing Modrinth only.{" "}
            <Link to="/settings" className="text-muted underline hover:text-accent">
              Add a CurseForge API key
            </Link>{" "}
            to include CurseForge.
          </p>
        )}
        {results?.warnings.map((warning) => (
          <p key={warning} className="text-[12.5px] text-warning">
            {warning}
          </p>
        ))}
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        {error ? (
          <EmptyState
            icon={<SearchX size={36} strokeWidth={1.5} />}
            title="Search failed"
            action={<Button onClick={() => void search(0)}>Try again</Button>}
          >
            {error}
          </EmptyState>
        ) : !results ? (
          <div className="flex h-full items-center justify-center text-muted">
            <Loader2 size={20} className="animate-spin" />
          </div>
        ) : results.hits.length === 0 ? (
          <EmptyState icon={<SearchX size={36} strokeWidth={1.5} />} title="No mods found">
            {matchVersion || fabricOnly
              ? `Nothing matches for Minecraft ${instance.mc_version} on Fabric. Try different words or loosen the filters.`
              : "Try different words or another category."}
          </EmptyState>
        ) : (
          <>
            <ul className="flex flex-col gap-2">
              {results.hits.map((hit) => (
                <ResultRow
                  key={`${hit.source}/${hit.project_id}`}
                  hit={hit}
                  instance={instance}
                  installed={installed ?? []}
                />
              ))}
            </ul>
            {results.hasMore && (
              <div className="mt-4 flex justify-center">
                <Button disabled={loading} onClick={() => void search(page + 1)}>
                  {loading ? "Loading…" : "Show more"}
                </Button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
