import { S } from "../strings";

/** The file names and sizes WordPress.org expects for plugin images. */
export function AssetGuide() {
  return (
    <div className="stack">
      <table className="table">
        <thead>
          <tr>
            <th scope="col">{S.tools.assets.columns.file}</th>
            <th scope="col">{S.tools.assets.columns.pixels}</th>
            <th scope="col">{S.tools.assets.columns.role}</th>
          </tr>
        </thead>
        <tbody>
          {S.tools.assets.guide.map((row) => (
            <tr key={row.name}>
              <td className="mono">{row.name}</td>
              <td className="mono">{row.size}</td>
              <td>{row.note}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="field__hint">{S.tools.assets.guideNote}</p>
    </div>
  );
}
