import { Icon } from "../icons";
import { TITLES, type ToolId } from "../lib/tools";

export function TopBar({ tool }: { tool: ToolId }) {
  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand__mark">
          <Icon name="paraphrase" size={16} />
        </span>
        <span className="brand__word">quillnot</span>
      </div>

      <h1 className="topbar__title">{TITLES[tool]}</h1>

      <div className="topbar__right">
        <span className="pill" title="Every model this app can call has a prompt price of zero on OpenRouter.">
          Free models
        </span>
        <span className="pill pill--mock" title="M1: results are produced locally. The OpenRouter client lands in M2.">
          Mock
        </span>
      </div>
    </header>
  );
}
