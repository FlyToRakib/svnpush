import { create } from "zustand";
import type { AdapterInfo } from "../ipc/bindings/AdapterInfo";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { ProviderInput } from "../ipc/bindings/ProviderInput";
import type { ProvidersFile } from "../ipc/bindings/ProvidersFile";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";

interface ProviderStore {
  adapters: AdapterInfo[];
  file: ProvidersFile;
  loaded: boolean;
  error: ErrorView | null;
  load: () => Promise<void>;
  save: (input: ProviderInput) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setDefault: (id: string) => Promise<void>;
  clearAttention: (id: string) => Promise<void>;
  saveFallback: (order: string[]) => Promise<void>;
}

const EMPTY: ProvidersFile = { schema: 1, providers: [], fallback: [] };

/** Provider records and adapter descriptions, as the shell returns them. */
export const useProviderStore = create<ProviderStore>((set) => {
  const apply = async (action: () => Promise<ProvidersFile>) => {
    const file = await action();
    set({ file, error: null });
  };

  return {
    adapters: [],
    file: EMPTY,
    loaded: false,
    error: null,

    load: async () => {
      try {
        const [adapters, file] = await Promise.all([
          commands.providerAdapters(),
          commands.listProviders(),
        ]);
        set({ adapters, file, loaded: true, error: null });
      } catch (error) {
        set({ loaded: true, error: toErrorView(error) });
      }
    },
    save: (input) => apply(() => commands.saveProvider(input)),
    remove: (id) => apply(() => commands.removeProvider(id)),
    setDefault: (id) => apply(() => commands.setDefaultProvider(id)),
    clearAttention: (id) => apply(() => commands.clearProviderAttention(id)),
    saveFallback: (order) => apply(() => commands.saveProviderFallback(order)),
  };
});
