import { ScreenHeader } from "../components/ScreenHeader";
import { S } from "../strings";

/** The Release screen. */
export function ReleaseScreen() {
  return <ScreenHeader title={S.release.title} subtitle={S.release.subtitle} />;
}
