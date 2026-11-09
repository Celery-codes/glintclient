import { useEffect, useState } from "react";
import { ipc, toAppError, type GameVersions, type LoaderVersion } from "./ipc";

/** Minecraft versions an instance can use. `error` is set if the list couldn't load. */
export function useGameVersions(includeSnapshots: boolean) {
  const [data, setData] = useState<GameVersions | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    ipc
      .listGameVersions(includeSnapshots)
      .then((versions) => {
        if (!cancelled) {
          setData(versions);
          setError(null);
        }
      })
      .catch((e) => !cancelled && setError(toAppError(e).message));
    return () => {
      cancelled = true;
    };
  }, [includeSnapshots]);
  return { data, error };
}

/** Fabric loader versions for a Minecraft version, newest first. */
export function useLoaderVersions(mcVersion: string | null) {
  const [state, setState] = useState<{ for: string; loaders: LoaderVersion[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!mcVersion) return;
    let cancelled = false;
    ipc
      .listLoaderVersions(mcVersion)
      .then((loaders) => {
        if (!cancelled) {
          setState({ for: mcVersion, loaders });
          setError(null);
        }
      })
      .catch((e) => !cancelled && setError(toAppError(e).message));
    return () => {
      cancelled = true;
    };
  }, [mcVersion]);
  // Never hand back another version's loaders while the new list is loading.
  const loaders = state && state.for === mcVersion ? state.loaders : null;
  return { loaders, error };
}

/** Newest stable loader, falling back to the newest of any kind. */
export function recommendedLoader(loaders: LoaderVersion[]): string | null {
  return (loaders.find((l) => l.stable) ?? loaders[0])?.version ?? null;
}
