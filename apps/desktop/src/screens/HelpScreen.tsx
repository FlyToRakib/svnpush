import { ScreenHeader } from "../components/ScreenHeader";
import { SetupChecklist } from "../components/SetupChecklist";
import type { Screen } from "./screen";
import { S } from "../strings";

interface HelpScreenProps {
  /** Shown on first launch, above the checklist. */
  welcome: boolean;
  onNavigate: (screen: Screen) => void;
}

/** Setup checklist, how a release works, and answers to common problems. */
export function HelpScreen({ welcome, onNavigate }: HelpScreenProps) {
  return (
    <>
      <ScreenHeader title={S.help.title} subtitle={S.help.subtitle} />
      <div className="stack help">
        {welcome && <p className="notice notice--info">{S.help.welcome}</p>}
        <section className="card">
          <div className="card__header">
            <h2 className="card__title">{S.help.setupTitle}</h2>
          </div>
          <div className="card__body">
            <SetupChecklist onNavigate={onNavigate} />
          </div>
        </section>
        <section className="card">
          <div className="card__header">
            <h2 className="card__title">{S.help.howTitle}</h2>
          </div>
          <div className="card__body stack">
            <ol className="help__steps">
              {S.help.how.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
            <p className="muted">{S.help.tip}</p>
          </div>
        </section>
        <section className="card">
          <div className="card__header">
            <h2 className="card__title">{S.help.filesTitle}</h2>
          </div>
          <div className="card__body">
            <p className="prose">{S.help.files}</p>
          </div>
        </section>
        <section className="card">
          <div className="card__header">
            <h2 className="card__title">{S.help.problemsTitle}</h2>
          </div>
          <div className="card__body">
            <dl className="help__problems">
              {S.help.problems.map((item) => (
                <div key={item.q}>
                  <dt>{item.q}</dt>
                  <dd>{item.a}</dd>
                </div>
              ))}
            </dl>
          </div>
        </section>
      </div>
    </>
  );
}
