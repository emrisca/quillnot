import { Icon } from "../icons";

/** The two views the right rail can open; Compare and Tone are still placeholders. */
export type SideView = "history" | "stats";

const ITEMS = [
  { key: "history", label: "History", icon: "history", view: "history" },
  { key: "stats", label: "Statistics", icon: "stats", view: "stats" },
  { key: "compare", label: "Compare", icon: "compare", soon: true },
  { key: "tone", label: "Tone", icon: "tone", soon: true },
] as const;

export function Rail({ onOpen }: { onOpen: (view: SideView) => void }) {
  return (
    <aside className="rail" aria-label="Views">
      {ITEMS.map((item) => (
        <button
          key={item.key}
          type="button"
          className="rail__item"
          title={"view" in item ? item.label : `${item.label} — a later milestone`}
          onClick={() => {
            if ("view" in item) onOpen(item.view);
          }}
        >
          <Icon name={item.icon} size={19} />
          <span>{item.label}</span>
        </button>
      ))}
    </aside>
  );
}
