import { Play, Square } from "lucide-react";
import { stageLabel } from "../lib/format";
import { useInstances, type Run } from "../stores/instances";
import { Button } from "./ui";

interface PlayButtonProps {
  id: string;
  size?: "sm" | "md";
  className?: string;
}

/** Play when idle; Cancel while installing; Stop while the game runs. */
export function PlayButton({ id, size = "md", className }: PlayButtonProps) {
  const run = useInstances((s) => s.runs[id]);
  const { play, stop } = useInstances.getState();

  if (!run) {
    return (
      <Button
        variant="primary"
        size={size}
        className={className}
        onClick={(e) => {
          e.stopPropagation();
          play(id);
        }}
        icon={<Play size={15} fill="currentColor" />}
      >
        Play
      </Button>
    );
  }
  return (
    <Button
      size={size}
      className={className}
      onClick={(e) => {
        e.stopPropagation();
        stop(id);
      }}
      icon={<Square size={13} fill="currentColor" />}
    >
      {run.state === "preparing" ? "Cancel" : "Stop"}
    </Button>
  );
}

/** One line describing what a busy instance is doing, with a bar while installing. */
export function RunStatus({ run }: { run: Run }) {
  if (run.state === "running") {
    return (
      <div className="flex items-center gap-2 text-[13px] text-success">
        <span className="h-2 w-2 rounded-full bg-success" />
        Running
      </div>
    );
  }
  const progress = run.progress;
  const fraction = progress && progress.total > 0 ? progress.done / progress.total : null;
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <div className="flex justify-between gap-3 text-[13px] text-muted">
        <span className="truncate">{progress ? stageLabel(progress.stage) : "Preparing"}…</span>
        {progress && progress.total > 1 && (
          <span className="shrink-0 tabular-nums">
            {progress.done} / {progress.total}
          </span>
        )}
      </div>
      <div className="h-1 overflow-hidden rounded-full bg-border">
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-150"
          style={{ width: `${Math.round((fraction ?? 0.04) * 100)}%` }}
        />
      </div>
    </div>
  );
}
