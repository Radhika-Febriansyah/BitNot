import { useCallback, useEffect, useState, type ReactNode } from "react";
import { AccentPicker, ThemePicker } from "../components/AppearanceControls";
import { ConfirmDialog, Dialog, PromptDialog } from "../components/Dialog";
import { ShortcutList } from "../components/ShortcutsDialog";
import { Icon } from "../components/Icon";
import { api, errorMessage } from "../lib/api";
import { folderCountText } from "../lib/folders";
import { t, tn, type MessageKey } from "../i18n";
import { scanSummaryText, type ScanIssue, type TrashItem, type TrashKind } from "../lib/library";
import { SCALE_OPTIONS } from "../lib/theme";
import type { LogFile, MotionPreference, Profile } from "../lib/types";
import { useApp } from "../state/AppContext";
import { useLibrary, useLibraryData } from "../state/LibraryContext";
import { useUi } from "../state/UiContext";

const MOTION: { value: MotionPreference; label: string }[] = [
  { value: "system", label: "Match Windows" },
  { value: "reduce", label: "Reduce motion" },
  { value: "full", label: "Full motion" },
];

function Section({ id, title, action, children }: { id: string; title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="settings-section" aria-labelledby={id}>
      <div className="section-head">
        <h2 id={id}>{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div className="setting-label">
        <h3>{label}</h3>
        {hint && <p className="muted">{hint}</p>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

function Appearance() {
  const { settings, updateSettings } = useApp();
  return (
    <Section id="appearance-title" title="Appearance">
      <Row label="Theme">
        <ThemePicker />
      </Row>
      <Row label="Accent colour" hint="Used for highlights and primary buttons.">
        <AccentPicker />
      </Row>
      <Row label="Display scale" hint="Makes text and controls larger or smaller.">
        <select
          className="select"
          aria-label="Display scale"
          value={settings.displayScale}
          onChange={(event) => void updateSettings({ displayScale: Number(event.target.value) })}
        >
          {SCALE_OPTIONS.map((scale) => (
            <option key={scale} value={scale}>
              {scale}%
            </option>
          ))}
        </select>
      </Row>
      <Row label={t("nowPlaying.layout")} hint={t("nowPlaying.layoutHint")}>
        <div className="chip-row" role="radiogroup" aria-label={t("nowPlaying.layout")}>
          {(["centered", "split"] as const).map((layout) => (
            <button
              key={layout}
              className="chip"
              role="radio"
              aria-checked={settings.nowPlayingLayout === layout}
              onClick={() => void updateSettings({ nowPlayingLayout: layout })}
            >
              {t(layout === "centered" ? "nowPlaying.centered" : "nowPlaying.split")}
            </button>
          ))}
        </div>
      </Row>
      <Row label="Motion" hint="Reduce motion turns off transitions and animations.">
        <div className="chip-row" role="radiogroup" aria-label="Motion">
          {MOTION.map((option) => (
            <button
              key={option.value}
              className="chip"
              role="radio"
              aria-checked={settings.reduceMotion === option.value}
              onClick={() => void updateSettings({ reduceMotion: option.value })}
            >
              {option.label}
            </button>
          ))}
        </div>
      </Row>
    </Section>
  );
}

function MusicFolders() {
  const { settings, updateSettings, notify } = useApp();
  const { scan, startScan, overview } = useLibrary();
  const { addFolder } = useUi();
  const [issues, setIssues] = useState<ScanIssue[] | null>(null);

  const change = (patch: Parameters<typeof updateSettings>[0]) =>
    void updateSettings(patch).then(startScan);
  const remove = (folder: string) =>
    change({ musicFolders: settings.musicFolders.filter((f) => f !== folder) });
  const showIssues = () =>
    api
      .scanIssues()
      .then(setIssues)
      .catch((error) => notify(errorMessage(error), "error"));

  const status = scan.running
    ? scan.total > 0
      ? `Scanning… ${scan.done} of ${scan.total} files checked.`
      : "Scanning…"
    : scan.summary
      ? scanSummaryText(scan.summary)
      : folderCountText(settings.musicFolders.length);

  return (
    <Section
      id="folders-title"
      title="Music folders"
      action={
        <div className="section-actions">
          <button
            className="btn"
            disabled={scan.running || settings.musicFolders.length === 0}
            onClick={() => void startScan()}
          >
            Rescan
          </button>
          <button className="btn" onClick={addFolder}>
            <Icon name="plus" size={1} />
            Add folder
          </button>
        </div>
      }
    >
      <p className="section-note" role="status">
        {status} BitNot only reads these folders. It never moves, changes, or deletes files in
        them.
      </p>
      {settings.musicFolders.length > 0 && (
        <ul className="rows">
          {settings.musicFolders.map((folder) => (
            <li key={folder} className="row">
              <span className="art row-icon">
                <Icon name="folder" size={1.125} />
              </span>
              <span className="row-main selectable">{folder}</span>
              <button
                className="btn btn-ghost"
                aria-label={`Remove ${folder} from BitNot`}
                title="Removes the folder from this list and its tracks from the library. The folder and its files stay on disk."
                onClick={() => remove(folder)}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <Row
        label="Include subfolders"
        hint="When off, only files directly inside each folder are added."
      >
        <div className="chip-row" role="radiogroup" aria-label="Include subfolders">
          <button
            className="chip"
            role="radio"
            aria-checked={!settings.scanSubfolders}
            onClick={() => settings.scanSubfolders && change({ scanSubfolders: false })}
          >
            Off
          </button>
          <button
            className="chip"
            role="radio"
            aria-checked={settings.scanSubfolders}
            onClick={() => !settings.scanSubfolders && change({ scanSubfolders: true })}
          >
            On
          </button>
        </div>
      </Row>
      {overview.issueCount > 0 && (
        <Row
          label={
            overview.issueCount === 1 ? "1 file was skipped" : `${overview.issueCount} files were skipped`
          }
          hint="Files that are damaged or in a format this version cannot play."
        >
          <button className="btn" onClick={() => void showIssues()}>
            View
          </button>
        </Row>
      )}

      <Dialog open={issues !== null} title="Skipped files" wide onClose={() => setIssues(null)}>
        <ul className="issue-list">
          {(issues ?? []).map((issue) => (
            <li key={issue.path}>
              <span className="selectable">{issue.path}</span>
              <span className="muted">{issue.reason}</span>
            </li>
          ))}
        </ul>
        <div className="dialog-actions">
          <button className="btn btn-primary btn-lg" onClick={() => setIssues(null)}>
            Close
          </button>
        </div>
      </Dialog>
    </Section>
  );
}

type ProfileDialog =
  | { kind: "create" }
  | { kind: "rename"; profile: Profile }
  | { kind: "delete"; profile: Profile }
  | null;

function Profiles() {
  const { boot, run, notify } = useApp();
  const [dialog, setDialog] = useState<ProfileDialog>(null);
  const [purging, setPurging] = useState<string | null>(null);
  const close = () => setDialog(null);

  return (
    <Section
      id="profiles-title"
      title="Profiles"
      action={
        <button className="btn" onClick={() => setDialog({ kind: "create" })}>
          <Icon name="plus" size={1} />
          New profile
        </button>
      }
    >
      <p className="section-note">
        Each profile has its own settings, library, playlists, favorites, and history.
        {boot.profiles.length > 1 &&
          " The main profile cannot be deleted; make another profile the main one first."}
      </p>
      <ul className="rows">
        {boot.profiles.map((profile) => {
          const active = profile.id === boot.activeProfileId;
          return (
            <li key={profile.id} className="row">
              <span className="avatar small" aria-hidden="true">
                {profile.name.charAt(0).toUpperCase()}
              </span>
              <span className="row-main">
                {profile.name}
                {profile.isMain && <span className="tag">Main</span>}
                {active && <span className="tag accent">Active</span>}
              </span>
              {!active && (
                <button className="btn" onClick={() => void run(() => api.switchProfile(profile.id))}>
                  Switch
                </button>
              )}
              <button className="btn btn-ghost" onClick={() => setDialog({ kind: "rename", profile })}>
                Rename
              </button>
              {!profile.isMain && (
                <>
                  <button
                    className="btn btn-ghost"
                    onClick={() => void run(() => api.setMainProfile(profile.id))}
                  >
                    Make main
                  </button>
                  <button
                    className="btn btn-ghost btn-danger"
                    onClick={() => setDialog({ kind: "delete", profile })}
                  >
                    Delete
                  </button>
                </>
              )}
            </li>
          );
        })}
      </ul>

      {boot.deletedProfiles.length > 0 && (
        <>
          <h3 className="subheading">Recently deleted</h3>
          <ul className="rows">
            {boot.deletedProfiles.map((profile) => (
              <li key={profile.id} className="row">
                <span className="art row-icon">
                  <Icon name="trash" size={1.125} />
                </span>
                <span className="row-main">
                  {profile.name}
                  <span className="muted">
                    {profile.daysRemaining === 1
                      ? "1 day left"
                      : `${profile.daysRemaining} days left`}
                  </span>
                </span>
                <button
                  className="btn"
                  onClick={() =>
                    void run(() => api.restoreProfile(profile.id)).then(
                      (ok) => ok && notify(`Restored ${profile.name}.`),
                    )
                  }
                >
                  Restore
                </button>
                <button className="btn btn-ghost btn-danger" onClick={() => setPurging(profile.id)}>
                  Delete now
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      <PromptDialog
        open={dialog?.kind === "create"}
        title="New profile"
        label="Profile name"
        submitLabel="Create"
        maxLength={40}
        onSubmit={(name) => void run(() => api.createProfile(name))}
        onClose={close}
      />
      <PromptDialog
        open={dialog?.kind === "rename"}
        title="Rename profile"
        label="Profile name"
        initialValue={dialog?.kind === "rename" ? dialog.profile.name : ""}
        submitLabel="Rename"
        maxLength={40}
        onSubmit={(name) => {
          if (dialog?.kind === "rename") {
            const id = dialog.profile.id;
            void run(() => api.renameProfile(id, name));
          }
        }}
        onClose={close}
      />
      <ConfirmDialog
        open={purging !== null}
        title="Permanently delete this profile?"
        message="Its settings, library, playlists, favorites, and history in BitNot are deleted for good. This cannot be undone. Your music files are not affected."
        confirmLabel="Delete permanently"
        danger
        onConfirm={() => {
          if (purging) {
            const id = purging;
            void run(() => api.purgeProfile(id));
          }
        }}
        onClose={() => setPurging(null)}
      />
      <ConfirmDialog
        open={dialog?.kind === "delete"}
        title={dialog?.kind === "delete" ? `Delete ${dialog.profile.name}?` : "Delete profile?"}
        message="The profile's BitNot data is kept for 90 days and can be restored from Settings until then. Your music files are never deleted."
        confirmLabel="Delete profile"
        danger
        onConfirm={() => {
          if (dialog?.kind === "delete") {
            const id = dialog.profile.id;
            void run(() => api.deleteProfile(id));
          }
        }}
        onClose={close}
      />
    </Section>
  );
}

const TRASH_KIND: Record<TrashKind, MessageKey> = {
  playlist: "trash.kindPlaylist",
  favorite: "trash.kindFavorite",
  track: "trash.kindTrack",
  metadata: "trash.kindMetadata",
};

/** Deleted playlists, removed tracks and favorites, and earlier details. */
function RecycleBin() {
  const { notify } = useApp();
  const { data: items } = useLibraryData(api.trash, [], ["trash"]);
  const [confirm, setConfirm] = useState<{ kind: "one"; item: TrashItem } | { kind: "all" } | null>(null);
  const fail = (error: unknown) => notify(errorMessage(error), "error");
  const list = items ?? [];

  return (
    <Section
      id="trash-title"
      title={t("trash.title")}
      action={
        <button className="btn btn-danger" disabled={list.length === 0} onClick={() => setConfirm({ kind: "all" })}>
          {t("trash.emptyAll")}
        </button>
      }
    >
      <p className="section-note">{t("trash.text")}</p>
      {items && list.length === 0 && <p className="section-note">{t("trash.empty")}</p>}
      {list.length > 0 && (
        <ul className="rows">
          {list.map((item) => (
            <li key={item.id} className="row">
              <span className="art row-icon">
                <Icon name="trash" size={1.125} />
              </span>
              <span className="row-main">
                {item.label}
                <span className="tag">{t(TRASH_KIND[item.kind])}</span>
                <span className="muted">{tn("count.daysLeft", item.daysRemaining)}</span>
              </span>
              <button className="btn" onClick={() => void api.restoreTrash(item.id).catch(fail)}>
                {t("trash.restore")}
              </button>
              <button className="btn btn-ghost btn-danger" onClick={() => setConfirm({ kind: "one", item })}>
                {t("trash.deleteNow")}
              </button>
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog
        open={confirm !== null}
        title={confirm?.kind === "all" ? t("trash.emptyTitle") : t("trash.purgeTitle")}
        message={confirm?.kind === "all" ? t("trash.emptyText") : t("trash.purgeText")}
        confirmLabel={confirm?.kind === "all" ? t("trash.emptyAll") : t("trash.deleteNow")}
        danger
        onConfirm={() => {
          if (confirm?.kind === "all") void api.emptyTrash().catch(fail);
          if (confirm?.kind === "one") void api.purgeTrash(confirm.item.id).catch(fail);
        }}
        onClose={() => setConfirm(null)}
      />
    </Section>
  );
}

function formatSize(bytes: number): string {
  return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;
}

function Diagnostics() {
  const { boot, notify } = useApp();
  const [logs, setLogs] = useState<LogFile[] | null>(null);
  const [viewing, setViewing] = useState<{ name: string; text: string } | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const refresh = useCallback(() => {
    api
      .listLogs()
      .then(setLogs)
      .catch((error) => notify(errorMessage(error), "error"));
  }, [notify]);

  useEffect(refresh, [refresh]);

  const view = (name: string) =>
    api
      .readLog(name)
      .then((text) => setViewing({ name, text }))
      .catch((error) => notify(errorMessage(error), "error"));

  const clear = () =>
    api
      .clearLogs()
      .then((count) => {
        notify(count === 1 ? "Deleted 1 log file." : `Deleted ${count} log files.`);
        refresh();
      })
      .catch((error) => notify(errorMessage(error), "error"));

  return (
    <Section
      id="diagnostics-title"
      title="Diagnostics"
      action={
        <button
          className="btn btn-danger"
          disabled={!logs || logs.length === 0}
          onClick={() => setConfirmClear(true)}
        >
          Delete all logs
        </button>
      }
    >
      <p className="section-note">
        Logs stay on this computer and are never sent anywhere. Logs older than 14 days are removed
        automatically.
        {boot.previousSessionCrashed && " BitNot did not close properly before this session."}
      </p>
      {logs && logs.length === 0 && <p className="section-note">No log files.</p>}
      {logs && logs.length > 0 && (
        <ul className="rows">
          {logs.map((log) => (
            <li key={log.name} className="row">
              <span className="row-main">
                {log.name}
                <span className="muted num">{formatSize(log.sizeBytes)}</span>
              </span>
              <button className="btn" onClick={() => void view(log.name)}>
                View
              </button>
            </li>
          ))}
        </ul>
      )}
      <Row label="Data location" hint="Where BitNot keeps its database, settings, and logs.">
        <code className="path selectable">{boot.dataDir}</code>
      </Row>

      <Dialog open={viewing !== null} title={viewing?.name ?? ""} wide onClose={() => setViewing(null)}>
        <pre className="log-view selectable" tabIndex={0}>
          {viewing?.text || "This log is empty."}
        </pre>
        <div className="dialog-actions">
          <button className="btn btn-primary" onClick={() => setViewing(null)}>
            Close
          </button>
        </div>
      </Dialog>
      <ConfirmDialog
        open={confirmClear}
        title="Delete all logs?"
        message="This permanently deletes BitNot's diagnostic logs. It does not affect your settings or music."
        confirmLabel="Delete logs"
        danger
        onConfirm={() => void clear()}
        onClose={() => setConfirmClear(false)}
      />
    </Section>
  );
}

export function Settings() {
  const { boot } = useApp();
  return (
    <div className="page settings">
      <h1>Settings</h1>
      <Appearance />
      <Section id="shortcuts-title" title={t("shortcuts.title")}>
        <p className="section-note">
          {t("shortcuts.note")} {t("shortcuts.settingsHint")}
        </p>
        <ShortcutList />
      </Section>
      <MusicFolders />
      <Profiles />
      <RecycleBin />
      <Diagnostics />
      <Section id="about-title" title="About">
        <p className="section-note">
          BitNot {boot.appVersion} · Your Music. Your Way. · Apache License 2.0
        </p>
      </Section>
    </div>
  );
}
