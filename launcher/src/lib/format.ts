export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

/** "4 GB", "6.5 GB" */
export function formatMemory(mb: number): string {
  const gb = mb / 1024;
  return `${Number.isInteger(gb) ? gb : gb.toFixed(1)} GB`;
}

/** "Just now", "12 minutes ago", "3 days ago", or a date once it's been a while. */
export function formatRelative(unixSeconds: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor(now / 1000 - unixSeconds));
  if (seconds < 60) return "Just now";
  const units: [number, string][] = [
    [60, "minute"],
    [3600, "hour"],
    [86400, "day"],
  ];
  if (seconds >= 30 * 86400) {
    return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  }
  let label = "";
  for (const [size, name] of units) {
    if (seconds >= size) {
      const n = Math.floor(seconds / size);
      label = `${n} ${name}${n === 1 ? "" : "s"} ago`;
    }
  }
  return label;
}

const STAGE_LABELS: Record<string, string> = {
  java: "Installing Java",
  libraries: "Downloading game files",
  assets: "Downloading assets",
};

export function stageLabel(stage: string): string {
  return STAGE_LABELS[stage] ?? "Preparing";
}

/** "1.2K", "250M" */
export function formatCount(n: number): string {
  if (n < 1000) return String(n);
  const units: [number, string][] = [
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "K"],
  ];
  for (const [size, suffix] of units) {
    if (n >= size) {
      const value = n / size;
      return `${value >= 100 ? Math.round(value) : value.toFixed(1).replace(/\.0$/, "")}${suffix}`;
    }
  }
  return String(n);
}
