// Game output arrives many times a second, so it lives outside React state: mutable
// per-instance buffers with a version counter that components subscribe to.
import { useSyncExternalStore } from "react";
import type { LogLine } from "../lib/ipc";

export type Level = "info" | "warn" | "error" | "debug";

export interface LogEntry {
  text: string;
  level: Level;
}

/** Matches the backend's buffer size. */
const CAPACITY = 5000;
const HEADER = /^\[\d\d:\d\d:\d\d\] \[[^\]]*\/(INFO|WARN|ERROR|FATAL|DEBUG|TRACE)\]/;

const buffers = new Map<string, LogEntry[]>();
const versions = new Map<string, number>();
const listeners = new Map<string, Set<() => void>>();
const EMPTY: LogEntry[] = [];

function levelOf(text: string, previous: Level): Level {
  const match = HEADER.exec(text);
  // Lines without a header (stack traces, mod lists) belong to the line above them.
  if (!match) return previous;
  switch (match[1]) {
    case "WARN":
      return "warn";
    case "ERROR":
    case "FATAL":
      return "error";
    case "DEBUG":
    case "TRACE":
      return "debug";
    default:
      return "info";
  }
}

function notify(id: string) {
  versions.set(id, (versions.get(id) ?? 0) + 1);
  listeners.get(id)?.forEach((listener) => listener());
}

export function appendLog(id: string, lines: LogLine[]) {
  let buffer = buffers.get(id);
  if (!buffer) {
    buffer = [];
    buffers.set(id, buffer);
  }
  let level: Level = buffer.at(-1)?.level ?? "info";
  for (const line of lines) {
    level = levelOf(line.text, level);
    buffer.push({ text: line.text, level });
  }
  if (buffer.length > CAPACITY) {
    buffer.splice(0, buffer.length - CAPACITY);
  }
  notify(id);
}

export function setLog(id: string, lines: LogLine[]) {
  buffers.set(id, []);
  appendLog(id, lines);
}

export function clearLog(id: string) {
  buffers.delete(id);
  notify(id);
}

export function hasLog(id: string): boolean {
  return (buffers.get(id)?.length ?? 0) > 0;
}

/** The buffer is mutated in place; `version` changes whenever its contents do. */
export function useLog(id: string): { entries: LogEntry[]; version: number } {
  const version = useSyncExternalStore(
    (listener) => {
      let set = listeners.get(id);
      if (!set) {
        set = new Set();
        listeners.set(id, set);
      }
      set.add(listener);
      return () => set.delete(listener);
    },
    () => versions.get(id) ?? 0,
  );
  return { entries: buffers.get(id) ?? EMPTY, version };
}
