/**
 * Every user-visible string in SVNpush. Short, factual, second person.
 * Buttons are verbs. No exclamation marks.
 */
export const S = {
  app: {
    name: "SVNpush",
  },
  nav: {
    label: "Main",
    projects: "Projects",
    release: "Release",
    providers: "Providers",
    vault: "Vault",
    settings: "Settings",
  },
  theme: {
    label: "Theme",
    system: "System",
    light: "Light",
    dark: "Dark",
  },
  projects: {
    title: "Projects",
    subtitle: "Your WordPress plugins. Open one to release it.",
  },
  release: {
    title: "Release",
    subtitle: "Open a project to release it.",
  },
  providers: {
    title: "AI Providers",
    subtitle: "Connect the AI that writes your release notes. You choose which runs, always.",
  },
  vault: {
    title: "Vault",
    subtitle: "SVN accounts, stored in your operating system keychain.",
  },
  settings: {
    title: "Settings",
    subtitle: "Appearance, tools and privacy.",
  },
} as const;
