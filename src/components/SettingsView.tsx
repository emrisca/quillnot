import { Icon } from "../icons";

const VERSION = "0.1.0";

/**
 * The sections that will hold real controls, laid out now so the shape is settled.
 * Each row says which milestone wires it rather than pretending to be disabled for
 * no reason — a greyed button with no explanation is worse than a labelled one.
 */
const SECTIONS = [
  {
    title: "OpenRouter",
    icon: "key",
    rows: [
      {
        label: "API key",
        detail: "Stored in the macOS Keychain / Windows Credential Manager and read only by the Rust side, so the window never holds the secret.",
        action: "Add key",
        milestone: "M2",
      },
      {
        label: "Model",
        detail: "Fetched from /api/v1/models and filtered to models whose prompt price is zero. Falls back to the next free model when one refuses.",
        action: "Choose",
        milestone: "M2",
      },
      {
        label: "Remaining requests",
        detail: "Free models are rate-limited per account. The count comes back on every reply and is shown in the header.",
        action: "—",
        milestone: "M2",
      },
    ],
  },
  {
    title: "Updates",
    icon: "download",
    rows: [
      {
        label: "Version",
        detail: `QuillNot ${VERSION}`,
        action: "Check for updates",
        milestone: "M5",
      },
      {
        label: "Channel",
        detail: "Reads latest.json from this repository's GitHub releases, signed with the release key.",
        action: "Stable",
        milestone: "M5",
      },
    ],
  },
  {
    title: "Data",
    icon: "stats",
    rows: [
      {
        label: "History",
        detail: "Every run is kept locally so you can reopen it. Nothing is uploaded and there is no account.",
        action: "—",
        milestone: "M4",
      },
      {
        label: "Statistics",
        detail: "Words processed, runs per mode, models used — from the local history, not from a server.",
        action: "—",
        milestone: "M4",
      },
    ],
  },
] as const;

export function SettingsView() {
  return (
    <div className="settings">
      <div className="settings__inner">
        {SECTIONS.map((section) => (
          <section key={section.title} className="card">
            <header className="card__head">
              <span className="card__icon">
                <Icon name={section.icon} size={17} />
              </span>
              <h2>{section.title}</h2>
            </header>

            {section.rows.map((row) => (
              <div key={row.label} className="row">
                <div className="row__text">
                  <span className="row__label">{row.label}</span>
                  <span className="row__detail">{row.detail}</span>
                </div>
                <div className="row__control">
                  <span className="chip chip--milestone">{row.milestone}</span>
                  <span className="row__action">{row.action}</span>
                </div>
              </div>
            ))}
          </section>
        ))}

        <p className="settings__note">
          QuillNot is a workspace of its own: it reads and writes your text and calls the model you pick, nothing else.
        </p>
      </div>
    </div>
  );
}
