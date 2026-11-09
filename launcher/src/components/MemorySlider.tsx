import { AlertTriangle, Info } from "lucide-react";
import { cx, formatMemory } from "../lib/format";
import type { MemoryLimits } from "../lib/ipc";

interface Advice {
  level: "danger" | "warning" | "info";
  message: string;
}

/** Thresholds come from the backend so the launcher has one definition of "too much". */
export function memoryAdvice(mb: number, limits: MemoryLimits): Advice | null {
  if (mb < limits.min_mb) {
    return {
      level: "danger",
      message: `Under ${formatMemory(limits.min_mb)}, current Minecraft versions stutter or run out of memory.`,
    };
  }
  if (mb > limits.max_recommended_mb) {
    return {
      level: "warning",
      message: `That's more than half of this PC's ${formatMemory(limits.total_mb)}. Windows and other apps need the rest, and the game slows down badly if memory runs short.`,
    };
  }
  if (mb > limits.diminishing_mb) {
    return {
      level: "info",
      message: `Beyond ${formatMemory(limits.diminishing_mb)} the client rarely gets faster, and garbage-collection pauses can get longer.`,
    };
  }
  return null;
}

interface MemorySliderProps {
  id?: string;
  value: number;
  limits: MemoryLimits;
  onChange: (mb: number) => void;
}

export function MemorySlider({ id, value, limits, onChange }: MemorySliderProps) {
  const advice = memoryAdvice(value, limits);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-4">
        <input
          id={id}
          type="range"
          min={1024}
          max={limits.slider_max_mb}
          step={512}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="h-9 flex-1"
        />
        <span className="w-20 text-right font-semibold tabular-nums">{formatMemory(value)}</span>
      </div>
      {advice && (
        <p
          className={cx(
            "flex items-start gap-2 text-[12.5px] leading-snug",
            advice.level === "danger" && "text-danger",
            advice.level === "warning" && "text-warning",
            advice.level === "info" && "text-muted",
          )}
        >
          {advice.level === "info" ? (
            <Info size={14} className="mt-px shrink-0" />
          ) : (
            <AlertTriangle size={14} className="mt-px shrink-0" />
          )}
          {advice.message}
        </p>
      )}
    </div>
  );
}
