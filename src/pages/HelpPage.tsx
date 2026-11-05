import { useStore } from "../store";
import type { TKey } from "../lib/i18n";

const topics: [TKey, TKey][] = [
  ["help.unauth.title", "help.unauth.body"],
  ["help.offline.title", "help.offline.body"],
  ["help.notdetected.title", "help.notdetected.body"],
  ["help.input.title", "help.input.body"],
  ["help.protected.title", "help.protected.body"],
];

export function HelpPage() {
  const { t, info } = useStore();
  return (
    <div className="page page--narrow">
      <header className="page__head">
        <div>
          <h1>{t("help.title")}</h1>
          <p>{t("help.subtitle")}</p>
        </div>
      </header>

      <section className="card">
        <h3>{t("help.android.title")}</h3>
        <ol className="steps">
          <li>{t("help.android.1")}</li>
          <li>{t("help.android.2")}</li>
          <li>{t("help.android.3")}</li>
        </ol>
      </section>

      <div className="help-grid">
        {topics.map(([title, body]) => (
          <section className="card" key={title}>
            <h3>{t(title)}</h3>
            <p className="muted">{t(body)}</p>
          </section>
        ))}
        <section className="card">
          <h3>{t("help.ios.title")}</h3>
          <p className="muted">{info?.iosCanMirror ? t("help.ios.mac") : t("help.ios.win")}</p>
        </section>
      </div>

      {info?.logDir && (
        <p className="footnote">
          {t("help.logs")}: <span className="mono">{info.logDir}</span>
        </p>
      )}
    </div>
  );
}
