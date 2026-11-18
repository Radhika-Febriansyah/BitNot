import { api, errorMessage } from "./api";
import type { Settings } from "./types";

type Notify = (message: string, kind?: "info" | "error") => void;

/**
 * Opens the folder picker and stores the chosen music folder.
 * Resolves to true when a folder was added (so the caller can scan).
 */
export async function addMusicFolder(
  settings: Settings,
  updateSettings: (patch: Partial<Settings>) => Promise<void>,
  notify: Notify,
): Promise<boolean> {
  let folder: string | null;
  try {
    folder = await api.pickFolder();
  } catch (error) {
    notify(errorMessage(error), "error");
    return false;
  }
  if (!folder) return false;
  const picked = folder.toLowerCase();
  if (settings.musicFolders.some((existing) => existing.toLowerCase() === picked)) {
    notify("That folder is already in your list.");
    return false;
  }
  await updateSettings({ musicFolders: [...settings.musicFolders, folder] });
  return true;
}

export function folderCountText(count: number): string {
  if (count === 0) return "No music folders selected.";
  return count === 1 ? "1 music folder selected." : `${count} music folders selected.`;
}
