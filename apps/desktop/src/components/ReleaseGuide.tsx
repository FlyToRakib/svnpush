import { useState } from "react";
import { S } from "../strings";

const KEY = "svnpush.releaseGuideHidden";

function readHidden(): boolean {
  try {
    return window.localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

function writeHidden(hidden: boolean) {
  try {
    window.localStorage.setItem(KEY, hidden ? "1" : "0");
  } catch {
    // Remembering the choice is a convenience; the guide works without it.
  }
}

/** The release workflow in four short steps, above the project page's sections. */
export function ReleaseGuide() {
  const [hidden, setHidden] = useState(readHidden);
  const toggle = () => {
    writeHidden(!hidden);
    setHidden(!hidden);
  };

  if (hidden) {
    return (
      <div>
        <button type="button" className="btn btn--sm btn--ghost" onClick={toggle}>
          {S.tools.flow.show}
        </button>
      </div>
    );
  }

  return (
    <section className="card" aria-labelledby="release-guide">
      <div className="card__header">
        <h2 id="release-guide" className="card__title">
          {S.tools.flow.title}
        </h2>
        <button type="button" className="btn btn--sm btn--ghost" onClick={toggle}>
          {S.tools.flow.hide}
        </button>
      </div>
      <div className="card__body stack">
        <ol className="release-guide">
          {S.tools.flow.steps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
        <p className="muted">{S.tools.flow.assetsNote}</p>
      </div>
    </section>
  );
}
