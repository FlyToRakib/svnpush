import { ScreenHeader } from "../components/ScreenHeader";
import { S } from "../strings";

/** The Vault screen. */
export function VaultScreen() {
  return <ScreenHeader title={S.vault.title} subtitle={S.vault.subtitle} />;
}
