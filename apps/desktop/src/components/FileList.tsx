import { openPath } from "../opener";
import { formatBytes } from "../format";
import type { Package } from "../ipc/bindings/Package";
import { S } from "../strings";
import { CopyButton } from "./CopyButton";

interface FileListProps {
  pkg: Package;
}

/** Step 5: the staged files with sizes, the zip and its checksum. */
export function FileList({ pkg }: FileListProps) {
  return (
    <div className="stack">
      <dl className="facts">
        <dt>{S.build.zip}</dt>
        <dd className="mono break">{pkg.zip_path}</dd>
        <dt>{S.build.checksum}</dt>
        <dd className="mono break">{pkg.sha256}</dd>
      </dl>
      <div className="row">
        <button
          type="button"
          className="btn btn--sm"
          onClick={() => {
            void openPath(pkg.zip_path);
          }}
        >
          {S.build.reveal}
        </button>
        <CopyButton text={pkg.zip_path} label={S.build.copyPath} />
      </div>
      <table className="table files">
        <caption className="table__caption">
          {S.build.total(pkg.files.length, formatBytes(pkg.total_size))}
        </caption>
        <thead>
          <tr>
            <th scope="col">{S.build.files}</th>
            <th scope="col" className="num">
              {S.build.size}
            </th>
          </tr>
        </thead>
        <tbody>
          {pkg.files.map((file) => (
            <tr key={file.rel}>
              <td className="mono">{file.rel}</td>
              <td className="mono num">{formatBytes(file.size)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
