import { t } from "../i18n";
import { SHORTCUTS } from "../lib/shortcuts";
import { Dialog } from "./Dialog";

/** The list of keyboard shortcuts, as a table of action and keys. */
export function ShortcutList() {
  return (
    <table className="shortcut-table">
      <caption className="visually-hidden">{t("shortcuts.title")}</caption>
      <thead className="visually-hidden">
        <tr>
          <th scope="col">{t("shortcuts.action")}</th>
          <th scope="col">{t("shortcuts.keys")}</th>
        </tr>
      </thead>
      <tbody>
        {SHORTCUTS.map((shortcut) => (
          <tr key={shortcut.action}>
            <th scope="row">{t(shortcut.label)}</th>
            <td>
              {shortcut.keys.map((combo, index) => (
                <span key={combo.join("+")} className="shortcut-combo">
                  {index > 0 && <span className="muted"> {t("shortcuts.or")} </span>}
                  {combo.map((part, partIndex) => (
                    <span key={part}>
                      {partIndex > 0 && <span aria-hidden="true">+</span>}
                      <kbd>{part}</kbd>
                    </span>
                  ))}
                </span>
              ))}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function ShortcutsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  return (
    <Dialog open={open} title={t("shortcuts.title")} onClose={onClose}>
      <p className="muted">{t("shortcuts.note")}</p>
      <ShortcutList />
      <div className="dialog-actions">
        <button className="btn btn-primary btn-lg" onClick={onClose} autoFocus>
          {t("details.close")}
        </button>
      </div>
    </Dialog>
  );
}
