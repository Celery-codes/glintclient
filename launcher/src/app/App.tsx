import { useEffect } from "react";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import { SignInDialog } from "../components/SignInDialog";
import { Toasts } from "../components/Toasts";
import { toAppError } from "../lib/ipc";
import { Home } from "../pages/Home";
import { InstanceDetail } from "../pages/InstanceDetail";
import { Instances } from "../pages/Instances";
import { Mods } from "../pages/Mods";
import { Skins } from "../pages/Placeholder";
import { Settings } from "../pages/Settings";
import { useAccounts } from "../stores/accounts";
import { startEvents } from "../stores/events";
import { useInstances } from "../stores/instances";
import { useSystem } from "../stores/system";
import { toast } from "../stores/toasts";
import { Sidebar } from "./Sidebar";

export function App() {
  useEffect(() => {
    startEvents();
    const report = (e: unknown) => toast.error(toAppError(e).message);
    useSystem.getState().load().catch(report);
    useAccounts.getState().load().catch(report);
    useInstances.getState().load().catch(report);
  }, []);

  return (
    // Hash routing: the app is served from a custom protocol with no server to
    // answer deep links.
    <HashRouter>
      <div className="flex h-full">
        <Sidebar />
        <main className="min-w-0 flex-1">
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/instances" element={<Instances />} />
            <Route path="/instances/:id" element={<InstanceDetail />} />
            <Route path="/mods" element={<Mods />} />
            <Route path="/skins" element={<Skins />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </main>
      </div>
      <SignInDialog />
      <Toasts />
    </HashRouter>
  );
}
