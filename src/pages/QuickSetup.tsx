import { useState, type CSSProperties } from "react";
import { AccentPicker, ThemePicker } from "../components/AppearanceControls";
import { Icon } from "../components/Icon";
import { Mark } from "../components/Logo";
import { api } from "../lib/api";
import { addMusicFolder } from "../lib/folders";
import { t } from "../i18n";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";

const LANGUAGE_NAMES: Record<string, string> = { en: "English" };
const STEPS = ["Language", "Theme", "Music folder", "Sound", "Finish"] as const;

/** First-run setup. Every step is optional and can be changed later in Settings. */
export function QuickSetup() {
  const { boot, settings, updateSettings, notify, run } = useApp();
  const { volume, setVolume } = useLibrary();
  const [step, setStep] = useState(0);
  const last = step === STEPS.length - 1;
  const finish = () => void run(api.completeSetup);

  return (
    <main className="setup">
      <div className="setup-card" aria-labelledby="setup-title">
        <div className="setup-top">
          <Mark size={2.5} />
          <ol className="setup-steps" aria-label="Setup progress">
            {STEPS.map((name, index) => (
              <li
                key={name}
                className="setup-step"
                aria-current={index === step ? "step" : undefined}
                data-done={index < step}
              >
                {name}
              </li>
            ))}
          </ol>
        </div>

        <div className="setup-body" key={step}>
          {step === 0 && (
            <>
              <h1 id="setup-title">Welcome to BitNot</h1>
              <p className="muted lead">Your Music. Your Way. Let's get a few basics in place.</p>
              <h3>Language</h3>
              <div className="chip-row" role="radiogroup" aria-label="Language">
                {boot.supportedLanguages.map((code) => (
                  <button
                    key={code}
                    className="chip"
                    role="radio"
                    aria-checked={settings.language === code}
                    onClick={() => void updateSettings({ language: code })}
                  >
                    {LANGUAGE_NAMES[code] ?? code}
                  </button>
                ))}
              </div>
              {boot.supportedLanguages.length === 1 && (
                <p className="muted">English is the only language available in this version.</p>
              )}
            </>
          )}

          {step === 1 && (
            <>
              <h1 id="setup-title">Make it yours</h1>
              <p className="muted lead">Changes preview straight away.</p>
              <h3>Theme</h3>
              <ThemePicker />
              <h3>Accent colour</h3>
              <AccentPicker />
            </>
          )}

          {step === 2 && (
            <>
              <h1 id="setup-title">Where is your music?</h1>
              <p className="muted lead">
                Choose a folder that holds your music. BitNot never moves, changes, or deletes your
                files.
              </p>
              {settings.musicFolders.length > 0 && (
                <ul className="rows">
                  {settings.musicFolders.map((folder) => (
                    <li key={folder} className="row">
                      <Icon name="folder" />
                      <span className="row-main selectable">{folder}</span>
                      <button
                        className="btn btn-ghost btn-lg"
                        aria-label={`Remove ${folder} from BitNot`}
                        onClick={() =>
                          void updateSettings({
                            musicFolders: settings.musicFolders.filter((f) => f !== folder),
                          })
                        }
                      >
                        Remove
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div>
                <button
                  className="btn btn-lg"
                  onClick={() => void addMusicFolder(settings, updateSettings, notify)}
                >
                  <Icon name="folder" size={1.125} />
                  {settings.musicFolders.length > 0 ? "Add another folder" : "Choose folder"}
                </button>
              </div>
              <p className="muted">
                Your music is added to the library when setup finishes. Only files directly inside
                the folder are added unless you turn on subfolders in Settings.
              </p>
            </>
          )}

          {step === 3 && (
            <>
              <h1 id="setup-title">{t("setup.audioTitle")}</h1>
              <p className="muted lead">{t("setup.audioLead")}</p>
              <label className="setup-volume">
                <span>{t("setup.volume")}</span>
                <input
                  type="range"
                  className="slider"
                  min={0}
                  max={100}
                  value={volume}
                  aria-valuetext={`${volume}%`}
                  style={{ "--fill": `${volume}%` } as CSSProperties}
                  onChange={(event) => setVolume(Number(event.target.value))}
                />
                <span className="num">{volume}%</span>
              </label>
            </>
          )}

          {step === 4 && (
            <>
              <h1 id="setup-title">You're all set</h1>
              <p className="muted lead">
                Everything you chose can be changed later in Settings. No account is needed.
              </p>
            </>
          )}
        </div>

        <div className="setup-actions">
          {step > 0 && (
            <button className="btn btn-ghost btn-lg" onClick={() => setStep(step - 1)}>
              Back
            </button>
          )}
          <span className="spacer" />
          {!last && (
            <button className="btn btn-ghost btn-lg" onClick={finish}>
              Skip setup
            </button>
          )}
          <button
            className="btn btn-primary btn-lg"
            onClick={last ? finish : () => setStep(step + 1)}
            autoFocus
          >
            {last ? "Open BitNot" : "Continue"}
          </button>
        </div>
      </div>
    </main>
  );
}
