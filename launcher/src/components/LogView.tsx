import { useVirtualizer } from "@tanstack/react-virtual";
import { Copy, ScrollText } from "lucide-react";
import { useEffect, useRef } from "react";
import { cx } from "../lib/format";
import { ipc } from "../lib/ipc";
import { hasLog, setLog, useLog, type Level } from "../stores/logs";
import { toast } from "../stores/toasts";
import { Button, EmptyState } from "./ui";

const ROW_HEIGHT = 19;

const LEVEL_CLASS: Record<Level, string> = {
  info: "text-text/85",
  warn: "text-warning",
  error: "text-danger",
  debug: "text-faint",
};

/** Live game output. Only the visible rows are in the DOM, so long logs stay cheap. */
export function LogView({ id }: { id: string }) {
  const { entries, version } = useLog(id);
  const scroller = useRef<HTMLDivElement>(null);
  // Follow new output only while the user is at the bottom; scrolling up pins the view.
  const follow = useRef(true);

  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 40,
  });

  // After a reload the frontend has no log but the backend may: fetch what it kept.
  useEffect(() => {
    if (hasLog(id)) return;
    let cancelled = false;
    void ipc.instanceLog(id).then((lines) => {
      if (!cancelled && lines.length > 0 && !hasLog(id)) setLog(id, lines);
    });
    return () => {
      cancelled = true;
    };
  }, [id]);

  useEffect(() => {
    if (follow.current && entries.length > 0) {
      virtualizer.scrollToIndex(entries.length - 1, { align: "end" });
    }
  }, [version, entries.length, virtualizer]);

  if (entries.length === 0) {
    return (
      <EmptyState icon={<ScrollText size={32} strokeWidth={1.5} />} title="No output yet">
        The game's log appears here while it runs.
      </EmptyState>
    );
  }

  const copyAll = async () => {
    await navigator.clipboard.writeText(entries.map((e) => e.text).join("\n"));
    toast.success("Log copied");
  };

  return (
    <div className="relative h-full">
      <Button
        size="sm"
        className="absolute top-2 right-4 z-10 border border-border"
        onClick={copyAll}
        icon={<Copy size={14} />}
      >
        Copy
      </Button>
      <div
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < ROW_HEIGHT * 2;
        }}
        className="selectable h-full overflow-auto rounded-md border border-border bg-surface py-2 font-mono text-[12.5px]"
      >
        <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
          {virtualizer.getVirtualItems().map((row) => (
            <div
              key={row.key}
              className={cx("absolute left-0 px-3 whitespace-pre", LEVEL_CLASS[entries[row.index].level])}
              style={{ top: row.start, height: ROW_HEIGHT, lineHeight: `${ROW_HEIGHT}px` }}
            >
              {entries[row.index].text}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
