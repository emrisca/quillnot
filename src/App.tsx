import { useEffect, useState } from "react";
import { GrammarView } from "./components/GrammarView";
import { ModelsView } from "./components/ModelsView";
import { ParaphraseView } from "./components/ParaphraseView";
import { SettingsView } from "./components/SettingsView";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { refreshStatus } from "./lib/connection";
import type { ToolId } from "./lib/tools";

export default function App() {
  const [tool, setTool] = useState<ToolId>("paraphraser");

  // Once at startup, so the header knows whether this build has an account before
  // the first rewrite rather than after it.
  useEffect(() => {
    void refreshStatus();
  }, []);

  return (
    <div className="app">
      <Sidebar active={tool} onSelect={setTool} />
      <div className="main">
        <TopBar tool={tool} />
        {tool === "paraphraser" ? <ParaphraseView /> : null}
        {tool === "grammar" ? <GrammarView /> : null}
        {tool === "models" ? <ModelsView /> : null}
        {tool === "settings" ? <SettingsView /> : null}
      </div>
    </div>
  );
}
