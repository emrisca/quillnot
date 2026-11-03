import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { useConnection } from "../lib/connection";
import { isDesktop } from "../lib/openrouter";
import { onQuota, type Quota } from "../lib/quota";
import { toggleTheme, useTheme } from "../lib/theme";
import { TITLES, type ToolId } from "../lib/tools";

export function TopBar({ tool }: { tool: ToolId }) {
  const connection = useConnection();
  const theme = useTheme();
  const nextTheme = theme === "dark" ? "light" : "dark";

  // What the last call's provider said was left in the window. Absent until a call has
  // been made, because a number nobody reported is not a number worth showing.
  const [quota, setQuota] = useState<Quota | null>(null);

  useEffect(() => {
    if (!isDesktop()) return;
    let stop: (() => void) | undefined;
    void onQuota(setQuota).then((off) => {
      stop = off;
    });
    return () => stop?.();
  }, []);

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
        {quota && (quota.remaining !== null || quota.limit !== null) ? (
          <span
            className="pill"
            title={
              `${quota.provider} reported ${quota.remaining ?? "?"} requests left` +
              (quota.limit ? ` of ${quota.limit}` : "") +
              " in its current window. Free tiers reset this often."
            }
          >
            {quota.remaining ?? "?"}
            {quota.limit ? `/${quota.limit}` : ""} left
          </span>
        ) : null}
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
