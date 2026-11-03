import { useEffect, useState } from "react";
import { GrammarView } from "./components/GrammarView";
import { HistoryView } from "./components/HistoryView";
import { ModelsView } from "./components/ModelsView";
import { ParaphraseView } from "./components/ParaphraseView";
import { SettingsView } from "./components/SettingsView";
import { Sidebar } from "./components/Sidebar";
import { StatsView } from "./components/StatsView";
import { TopBar } from "./components/TopBar";
import { refreshStatus } from "./lib/connection";
import { RESTORE_EVENT, type Run } from "./lib/history";
import type { SideView } from "./components/Rail";
import type { ToolId } from "./lib/tools";

export default function App() {
  const [tool, setTool] = useState<ToolId>("paraphraser");
  // History and Statistics sit over the workspace rather than replacing a tool in the
  // sidebar, because that is where the rail offers them.
  const [view, setView] = useState<SideView | null>(null);

  // Once at startup, so the header knows whether this build has an account before
  // the first rewrite rather than after it.
  useEffect(() => {
    void refreshStatus();
  }, []);

  // Opening a run switches to the tool that has to show it. The run itself is parked
  // in `lib/history` because that tool mounts after this event, and would miss it.
  useEffect(() => {
    const onRestore = (event: Event) => {
      const run = (event as CustomEvent<Run>).detail;
      setView(null);
      setTool(run.tool === "grammar" ? "grammar" : "paraphraser");
    };
    window.addEventListener(RESTORE_EVENT, onRestore);
    return () => window.removeEventListener(RESTORE_EVENT, onRestore);
  }, []);

  return (
    <div className="app">
      <Sidebar
        active={tool}
        onSelect={(next) => {
          setView(null);
          setTool(next);
        }}
      />
      <div className="main">
        <TopBar tool={tool} />
        {view === "history" ? <HistoryView onClose={() => setView(null)} /> : null}
        {view === "stats" ? <StatsView onClose={() => setView(null)} /> : null}
        {view === null ? (
          <>
            {tool === "paraphraser" ? <ParaphraseView onOpen={setView} /> : null}
            {tool === "grammar" ? <GrammarView onOpen={setView} /> : null}
            {tool === "models" ? <ModelsView /> : null}
            {tool === "settings" ? <SettingsView onOpen={setView} /> : null}
          </>
        ) : null}
      </div>
    </div>
  );
}
