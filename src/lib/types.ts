// Shapes shared with the Rust core (see src-tauri/src/core.rs and settings.rs).

export type Theme = "dark" | "light" | "system";
export type MotionPreference = "system" | "reduce" | "full";
export type NowPlayingLayout = "centered" | "split";

export interface Settings {
  language: string;
  theme: Theme;
  accentColor: string;
  displayScale: number;
  reduceMotion: MotionPreference;
  musicFolders: string[];
  scanSubfolders: boolean;
  volume: number;
  recentSearches: string[];
  libraryView: "grid" | "list";
  /** Serialised route of the page that was open last. */
  lastPage: string;
  queuePanelOpen: boolean;
  sidebarCollapsed: boolean;
  guideDismissed: boolean;
  nowPlayingLayout: NowPlayingLayout;
}

export interface Profile {
  id: string;
  name: string;
  isMain: boolean;
  createdAt: number;
}

export interface DeletedProfile {
  id: string;
  name: string;
  deletedAt: number;
  daysRemaining: number;
}

export interface Bootstrap {
  appVersion: string;
  setupComplete: boolean;
  previousSessionCrashed: boolean;
  activeProfileId: string;
  profiles: Profile[];
  deletedProfiles: DeletedProfile[];
  settings: Settings;
  supportedLanguages: string[];
  dataDir: string;
  artworkDir: string;
  recoveryNotices: string[];
}

export interface LogFile {
  name: string;
  sizeBytes: number;
}

export interface AppErrorPayload {
  kind: string;
  message: string;
}
