import { useNavigate } from "react-router-dom";
import { formatRelative } from "../lib/format";
import type { Instance } from "../lib/ipc";
import { useInstances } from "../stores/instances";
import { PlayButton, RunStatus } from "./PlayButton";

export function InstanceCard({ instance }: { instance: Instance }) {
  const navigate = useNavigate();
  const run = useInstances((s) => s.runs[instance.id]);
  const open = () => navigate(`/instances/${instance.id}`);

  return (
    <div
      role="link"
      tabIndex={0}
      onClick={open}
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.target === e.currentTarget) open();
      }}
      className="flex cursor-pointer flex-col gap-4 rounded-lg border border-border bg-surface p-4 transition-colors hover:border-faint"
    >
      <div className="flex items-start gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-accent/14 text-[16px] font-semibold text-accent">
          {instance.name[0]?.toUpperCase()}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{instance.name}</div>
          <div className="truncate text-[13px] text-muted">
            {instance.mc_version} · Fabric {instance.loader_version}
          </div>
        </div>
      </div>

      <div className="flex h-9 items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          {run ? (
            <RunStatus run={run} />
          ) : (
            <span className="text-[13px] text-faint">
              {instance.last_played
                ? `Played ${formatRelative(instance.last_played).toLowerCase()}`
                : "Never played"}
            </span>
          )}
        </div>
        <PlayButton id={instance.id} size="sm" />
      </div>
    </div>
  );
}
