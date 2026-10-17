import { useEffect, useState } from "react";
import { useStore, startProcessPolling } from "./store";
import { Sidebar } from "./components/Sidebar";
import { Dashboard } from "./screens/Dashboard";
import { Sessions } from "./screens/Sessions";
import { Directories } from "./screens/Directories";
import { Backups } from "./screens/Backups";
import { Settings } from "./screens/Settings";
import { CleanWizard } from "./components/CleanWizard";
import type { CleanSelection } from "./lib/types";

export default function App() {
  const view = useStore((s) => s.view);
  const refresh = useStore((s) => s.refresh);
  const loadSettings = useStore((s) => s.loadSettings);
  const report = useStore((s) => s.report);
  const [wizardSel, setWizardSel] = useState<CleanSelection | null>(null);

  useEffect(() => {
    void loadSettings();
    void refresh();
    startProcessPolling();
  }, [loadSettings, refresh]);

  const onClean = (sel: CleanSelection) => setWizardSel(sel);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        {!report && view !== "settings" && view !== "backups" ? null : null}
        {view === "dashboard" && <Dashboard onClean={onClean} />}
        {view === "sessions" && <Sessions onClean={onClean} />}
        {view === "directories" && <Directories onClean={onClean} />}
        {view === "backups" && <Backups />}
        {view === "settings" && <Settings />}
      </main>
      <CleanWizard selection={wizardSel} onClose={() => setWizardSel(null)} />
    </div>
  );
}
