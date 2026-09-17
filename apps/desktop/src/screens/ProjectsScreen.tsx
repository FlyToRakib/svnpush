import { ScreenHeader } from "../components/ScreenHeader";
import { S } from "../strings";

/** The Projects screen. */
export function ProjectsScreen() {
  return <ScreenHeader title={S.projects.title} subtitle={S.projects.subtitle} />;
}
