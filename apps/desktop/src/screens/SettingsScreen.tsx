import { ScreenHeader } from "../components/ScreenHeader";
import { S } from "../strings";

/** The Settings screen. */
export function SettingsScreen() {
  return <ScreenHeader title={S.settings.title} subtitle={S.settings.subtitle} />;
}
