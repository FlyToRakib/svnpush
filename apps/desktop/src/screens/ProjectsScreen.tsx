import { useState } from "react";
import { AddProjectForm } from "../components/AddProjectForm";
import { ErrorNotice } from "../components/ErrorNotice";
import { ProjectRow } from "../components/ProjectRow";
import { ScreenHeader } from "../components/ScreenHeader";
import { useProjectStore } from "../store/projectStore";
import { S } from "../strings";

interface ProjectsScreenProps {
  onOpen: (path: string) => void;
}

/** The list of plugin projects. */
export function ProjectsScreen({ onOpen }: ProjectsScreenProps) {
  const { projects, loaded, error, add } = useProjectStore();
  const [adding, setAdding] = useState(false);

  const addButton = (
    <button
      type="button"
      className="btn btn--primary"
      onClick={() => {
        setAdding(true);
      }}
    >
      {S.projects.add}
    </button>
  );

  return (
    <>
      <ScreenHeader
        title={S.projects.title}
        subtitle={S.projects.subtitle}
        actions={!adding && addButton}
      />
      {error && <ErrorNotice error={error} />}
      {adding && (
        <AddProjectForm
          onCancel={() => {
            setAdding(false);
          }}
          onAdd={async (folder, svnUrl, mainFile) => {
            const summary = await add(folder, svnUrl, mainFile);
            setAdding(false);
            onOpen(summary.project.path);
          }}
        />
      )}
      {loaded && projects.length === 0 && !adding && (
        <div className="card">
          <div className="empty">
            <h2 className="empty__title">{S.projects.emptyTitle}</h2>
            <p className="empty__body">{S.projects.emptyBody}</p>
            {addButton}
          </div>
        </div>
      )}
      {projects.length > 0 && (
        <div className="card">
          <table className="table projects">
            <thead>
              <tr>
                <th scope="col">{S.projects.columns.name}</th>
                <th scope="col">{S.projects.columns.version}</th>
                <th scope="col">{S.projects.columns.lastRelease}</th>
                <th scope="col">
                  <span className="visually-hidden">{S.projects.open}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {projects.map((summary) => (
                <ProjectRow
                  key={summary.project.path}
                  summary={summary}
                  onOpen={() => {
                    onOpen(summary.project.path);
                  }}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
