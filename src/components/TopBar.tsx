import { Icon } from "../icons";
import { useConnection } from "../lib/connection";
import { toggleTheme, useTheme } from "../lib/theme";
import { TITLES, type ToolId } from "../lib/tools";

export function TopBar({ tool }: { tool: ToolId }) {
  const connection = useConnection();
  const theme = useTheme();
  const nextTheme = theme === "dark" ? "light" : "dark";

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
        <button
          type="button"
          className="iconbtn"
          onClick={toggleTheme}
          title={`Switch to ${nextTheme} mode`}
          aria-label={`Switch to ${nextTheme} mode`}
        >
          <Icon name={theme === "dark" ? "sun" : "moon"} size={17} />
        </button>
        <span
          className="pill"
          title="Every model this app calls has a prompt price of zero on OpenRouter."
        >
          Free models
        </span>
        {connection.connected ? (
          <span className="pill pill--live" title={`Signed in with ${connection.hint ?? "an OpenRouter key"}`}>
            Live
          </span>
        ) : (
          <span
            className="pill pill--mock"
            title="No account connected, so rewrites come from the local mock. Sign in from Settings."
          >
            Mock
          </span>
        )}
      </div>
    </header>
  );
}
