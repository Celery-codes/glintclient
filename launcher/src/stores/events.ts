import { listen } from "@tauri-apps/api/event";
import type {
  AuthFinishedEvent,
  InstanceLogEvent,
  InstanceProgressEvent,
  InstanceStateEvent,
} from "../lib/ipc";
import { useAccounts } from "./accounts";
import { useInstances } from "./instances";
import { appendLog } from "./logs";

let started = false;

/** Subscribe to backend events for the lifetime of the app. */
export function startEvents() {
  if (started) return;
  started = true;
  void listen<InstanceStateEvent>("instance-state", (e) =>
    useInstances.getState().onState(e.payload),
  );
  void listen<InstanceProgressEvent>("instance-progress", (e) =>
    useInstances.getState().onProgress(e.payload),
  );
  void listen<InstanceLogEvent>("instance-log", (e) => appendLog(e.payload.id, e.payload.lines));
  void listen<AuthFinishedEvent>("auth-finished", (e) =>
    useAccounts.getState().finishLogin(e.payload),
  );
}
