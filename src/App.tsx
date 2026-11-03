import { useState } from "react";
import { GrammarView } from "./components/GrammarView";
import { ParaphraseView } from "./components/ParaphraseView";
import { SettingsView } from "./components/SettingsView";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import type { ToolId } from "./lib/tools";

export default function App() {
  const [tool, setTool] = useState<ToolId>("paraphraser");

  return (
    <div className="app">
      <Sidebar active={tool} onSelect={setTool} />
      <div className="main">
        <TopBar tool={tool} />
        {tool === "paraphraser" ? <ParaphraseView /> : null}
        {tool === "grammar" ? <GrammarView /> : null}
        {tool === "settings" ? <SettingsView /> : null}
      </div>
    </div>
  );
}
