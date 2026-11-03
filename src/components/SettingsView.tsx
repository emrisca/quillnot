import { Icon } from "../icons";
import { ProviderList } from "./ProviderList";

const VERSION = "0.1.0";

const SECTIONS = [
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
        detail: "Every run is kept locally so you can reopen it. Nothing is uploaded and there is no account to make.",
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
        <ProviderList />

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
          QuillNot reads and writes your text and calls the provider you pick, nothing
          else. There is no QuillNot account and no server in the middle.
        </p>
      </div>
    </div>
  );
}
