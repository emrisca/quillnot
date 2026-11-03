import { Icon } from "../icons";
import { RAIL, RAIL_FOOTER, type ToolId } from "../lib/tools";

type Props = { active: ToolId; onSelect: (tool: ToolId) => void };

export function Sidebar({ active, onSelect }: Props) {
  const renderItem = (item: (typeof RAIL)[number]) => {
    const isActive = item.tool === active;
    const className = ["sidebar__item", isActive ? "is-active" : "", item.soon ? "is-soon" : ""]
      .filter(Boolean)
      .join(" ");

    return (
      <button
        key={item.key}
        type="button"
        className={className}
        aria-current={isActive ? "page" : undefined}
        title={item.soon ? `${item.label} (a later milestone)` : item.label}
        onClick={() => {
          if (item.tool) onSelect(item.tool);
        }}
      >
        <Icon name={item.icon} />
        <span>{item.label}</span>
      </button>
    );
  };

  return (
    <nav className="sidebar" aria-label="Tools">
      {RAIL.map(renderItem)}
      <div className="sidebar__spacer" />
      {RAIL_FOOTER.map(renderItem)}
    </nav>
  );
}
