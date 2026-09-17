import { create } from "zustand";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { ProjectSettings } from "../ipc/bindings/ProjectSettings";
import type { ProjectSummary } from "../ipc/bindings/ProjectSummary";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";

interface ProjectStore {
  projects: ProjectSummary[];
  loaded: boolean;
  error: ErrorView | null;
  selectedPath: string | null;
  load: () => Promise<void>;
  add: (folder: string, svnUrl: string, mainFile: string | null) => Promise<ProjectSummary>;
  update: (path: string, svnUrl: string, settings: ProjectSettings) => Promise<void>;
  remove: (path: string) => Promise<void>;
  select: (path: string | null) => void;
}

function replace(list: ProjectSummary[], next: ProjectSummary): ProjectSummary[] {
  const exists = list.some((p) => p.project.path === next.project.path);
  return exists
    ? list.map((p) => (p.project.path === next.project.path ? next : p))
    : [...list, next];
}

/** Projects and the one that is open. */
export const useProjectStore = create<ProjectStore>((set) => ({
  projects: [],
  loaded: false,
  error: null,
  selectedPath: null,

  load: async () => {
    try {
      const projects = await commands.listProjects();
      set({ projects, loaded: true, error: null });
    } catch (error) {
      set({ loaded: true, error: toErrorView(error) });
    }
  },

  add: async (folder, svnUrl, mainFile) => {
    const summary = await commands.addProject(folder, svnUrl, mainFile);
    set((s) => ({ projects: replace(s.projects, summary), error: null }));
    return summary;
  },

  update: async (path, svnUrl, settings) => {
    const summary = await commands.updateProject(path, svnUrl, settings);
    set((s) => ({ projects: replace(s.projects, summary) }));
  },

  remove: async (path) => {
    await commands.removeProject(path);
    set((s) => ({
      projects: s.projects.filter((p) => p.project.path !== path),
      selectedPath: s.selectedPath === path ? null : s.selectedPath,
    }));
  },

  select: (path) => {
    set({ selectedPath: path });
  },
}));
