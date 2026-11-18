import { useApp } from "../state/AppContext";
import { ACCENT_PRESETS } from "../lib/theme";
import type { Theme } from "../lib/types";
import { Icon } from "./Icon";

const THEMES: { value: Theme; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "Match Windows" },
];

/** Theme and accent pickers, shared by Quick Setup and Settings. Changes preview live. */
export function ThemePicker() {
  const { settings, updateSettings } = useApp();
  return (
    <div className="chip-row" role="radiogroup" aria-label="Theme">
      {THEMES.map((theme) => (
        <button
          key={theme.value}
          className="chip"
          role="radio"
          aria-checked={settings.theme === theme.value}
          onClick={() => void updateSettings({ theme: theme.value })}
        >
          {theme.label}
        </button>
      ))}
    </div>
  );
}

export function AccentPicker() {
  const { settings, updateSettings } = useApp();
  const current = settings.accentColor.toUpperCase();
  const isPreset = ACCENT_PRESETS.some((preset) => preset.value === current);

  return (
    <div className="swatch-row" role="radiogroup" aria-label="Accent colour">
      {ACCENT_PRESETS.map((preset) => (
        <button
          key={preset.value}
          className="swatch"
          role="radio"
          aria-checked={current === preset.value}
          aria-label={preset.name}
          title={preset.name}
          style={{ background: preset.value }}
          onClick={() => void updateSettings({ accentColor: preset.value })}
        >
          {current === preset.value && <Icon name="check" size={1} />}
        </button>
      ))}
      <label className="swatch custom" title="Custom colour" data-active={!isPreset}>
        <span className="visually-hidden">Custom accent colour</span>
        <input
          type="color"
          value={settings.accentColor}
          onChange={(event) => void updateSettings({ accentColor: event.target.value })}
        />
        <Icon name="plus" size={1} />
      </label>
    </div>
  );
}
