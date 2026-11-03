import type { SVGProps } from "react";

export type IconName =
  | "projects"
  | "paraphrase"
  | "grammar"
  | "detector"
  | "plagiarism"
  | "humanize"
  | "summarize"
  | "translate"
  | "more"
  | "settings"
  | "feedback"
  | "history"
  | "compare"
  | "stats"
  | "tone"
  | "trash"
  | "copy"
  | "check"
  | "close"
  | "sparkle"
  | "arrow"
  | "clock"
  | "key"
  | "download"
  | "warning"
  | "typer"
  | "models";

// Single-path icons: every glyph is one `d`, which keeps the component to a map
// and avoids an icon package for the twenty-odd marks the shell needs.
const PATHS: Record<IconName, string> = {
  projects:
    "M3 7.5A2.5 2.5 0 0 1 5.5 5h2.6l2 2.4h8.4A2.5 2.5 0 0 1 21 9.9v7.6A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5z",
  paraphrase: "M20.2 4.4a5 5 0 0 0-7 0l-7 7v5.4h5.4l7-7a5 5 0 0 0 1.6-5.4zM4 20l6.5-6.5",
  grammar: "M4 18 8.6 6l4.6 12M5.8 14H12M15.8 14.8l2.2 2.4 4-5",
  detector:
    "M4 8.5V6a2 2 0 0 1 2-2h2.5M20 8.5V6a2 2 0 0 0-2-2h-2.5M4 15.5V18a2 2 0 0 0 2 2h2.5M20 15.5V18a2 2 0 0 1-2 2h-2.5M8 12h8",
  plagiarism: "M12 3.2 19 6v5.8c0 4.1-2.9 7.2-7 9-4.1-1.8-7-4.9-7-9V6zM9.2 12.2l2 2 3.6-4",
  humanize:
    "M12 12.2a3.9 3.9 0 1 0 0-7.8 3.9 3.9 0 0 0 0 7.8zM4.6 21c.7-3.5 3.6-5.4 7.4-5.4s6.7 1.9 7.4 5.4",
  summarize: "M5 7h14M5 12h14M5 17h9",
  translate:
    "M4.5 5h7M8 3.2v1.9c0 4.1-2.2 7.1-5 8.9M6.4 9.6c1 2.1 3.2 3.9 5.9 4.9M13.4 20.8 17.6 11l4.1 9.8M14.9 17.9h5.2",
  more: "M12 6.2h.01M12 12h.01M12 17.8h.01",
  settings:
    "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4zM19.1 14.6a1.7 1.7 0 0 0 .4 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2v.2a2 2 0 1 1-4 0v-.2a1.7 1.7 0 0 0-2.9-1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0-1.2-2.9H3a2 2 0 1 1 0-4h.2a1.7 1.7 0 0 0 1.2-2.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 2.9-1.2V3a2 2 0 1 1 4 0v.2a1.7 1.7 0 0 0 2.9 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0 1.2 2.9h.2a2 2 0 1 1 0 4h-.2a1.7 1.7 0 0 0-1.6 1z",
  feedback: "M20 14.8a3 3 0 0 1-3 3H9.4L4 21V6.2a3 3 0 0 1 3-3h10a3 3 0 0 1 3 3z",
  history: "M3.6 12.4a8.4 8.4 0 1 0 2.5-6.6M3 4.4v4.6h4.6M12 8.2v4.4l3.1 1.9",
  compare:
    "M9.2 4H6.6A2.6 2.6 0 0 0 4 6.6v10.8A2.6 2.6 0 0 0 6.6 20h2.6M14.8 4h2.6A2.6 2.6 0 0 1 20 6.6v10.8a2.6 2.6 0 0 1-2.6 2.6h-2.6M12 3v18",
  stats: "M5 20v-8.6M12 20V4.6M19 20v-6",
  tone: "M4 12a8 8 0 1 1 16 0c0 3-2.2 5.2-5 5.2h-3L8 20.6v-3.4A7.3 7.3 0 0 1 4 12z",
  trash:
    "M4 7h16M9.4 7V4.8h5.2V7M6.6 7l.8 12a2 2 0 0 0 2 1.9h5.2a2 2 0 0 0 2-1.9L17.4 7M10.2 10.8v6.4M13.8 10.8v6.4",
  copy: "M8.4 8.4h9.2a1.8 1.8 0 0 1 1.8 1.8v9.2a1.8 1.8 0 0 1-1.8 1.8H8.4a1.8 1.8 0 0 1-1.8-1.8V10.2a1.8 1.8 0 0 1 1.8-1.8zM4.6 15.6A1.8 1.8 0 0 1 3 13.8V4.6A1.8 1.8 0 0 1 4.6 2.8h9.2A1.8 1.8 0 0 1 15.6 4.6v1.8",
  check: "M4.6 12.6l5 5L19.6 6.6",
  close: "M6 6l12 12M18 6 6 18",
  sparkle:
    "M11.6 3 13.4 8.2 18.6 10l-5.2 1.8L11.6 17l-1.8-5.2L4.6 10l5.2-1.8zM18 15.2l.7 1.9 1.9.7-1.9.7-.7 1.9-.7-1.9-1.9-.7 1.9-.7z",
  arrow: "M5 12h13M12.6 6.4 18.8 12l-6.2 5.6",
  clock: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7.4V12l3 1.8",
  key: "M16.4 8.6a3.4 3.4 0 1 1-3.5 3.3M13 12 4 21h3v-3h3v-3h2l1-1",
  download: "M12 3.8v11.4M7.8 11.4 12 15.6l4.2-4.2M5 20.2h14",
  warning: "M12 4.2 21 19H3zM12 9.6v4.2M12 16.6h.01",
  typer:
    "M3.6 7.6h16.8A1.6 1.6 0 0 1 22 9.2v5.6a1.6 1.6 0 0 1-1.6 1.6H3.6A1.6 1.6 0 0 1 2 14.8V9.2a1.6 1.6 0 0 1 1.6-1.6zM6 11h.01M9 11h.01M12 11h.01M15 11h.01M18 11h.01M7 14.4h10",
  models: "M12 3.4 21 8.1l-9 4.7-9-4.7zM3.4 12.1 12 16.5l8.6-4.4M3.4 15.9 12 20.3l8.6-4.4",
};

export function Icon({
  name,
  size = 20,
  ...rest
}: { name: IconName; size?: number } & Omit<SVGProps<SVGSVGElement>, "name">) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
