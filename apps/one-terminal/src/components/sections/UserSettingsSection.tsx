/**
 * UserSettingsSection
 *
 * App Menu drawer section for date / time / week-start preferences. Reads
 * and writes through `userSettingsStore`; live preview updates as the user
 * changes the selects because the store fires a `notify()` on every save.
 */

import { useEffect, useMemo, useState } from "react";
import {
  type DateFormat,
  type FirstDayOfWeek,
  type TimeFormat,
  type UserSettings,
  dayName,
  formatDate,
  formatTime,
  loadSettings,
  saveSettings,
  subscribe,
} from "../../settings/userSettingsStore";
import "./UserSettingsSection.css";

const DATE_OPTIONS: { value: DateFormat; label: string }[] = [
  { value: "iso", label: "ISO (2026-05-24)" },
  { value: "us", label: "US (05/24/2026)" },
  { value: "eu", label: "European (24/05/2026)" },
  { value: "long", label: "Long (May 24, 2026)" },
];

const TIME_OPTIONS: { value: TimeFormat; label: string }[] = [
  { value: "24h", label: "24-hour (15:45)" },
  { value: "12h", label: "12-hour (3:45 PM)" },
];

const FDOW_OPTIONS: { value: FirstDayOfWeek; label: string }[] = [
  { value: "monday", label: "Monday" },
  { value: "sunday", label: "Sunday" },
];

export function UserSettingsSection() {
  const [settings, setSettings] = useState<UserSettings>(() => loadSettings());

  // Re-render whenever any consumer writes via saveSettings (including this
  // component's own writes — keeps state in sync without a separate setState
  // path).
  useEffect(() => subscribe(setSettings), []);

  // Tick the preview every 30 s so the displayed clock reflects real time.
  // Cheap — only one date format render per tick.
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(t);
  }, []);

  const previewDate = useMemo(
    () => formatDate(now, settings.dateFormat),
    [now, settings.dateFormat]
  );
  const previewTime = useMemo(
    () => formatTime(now, settings.timeFormat),
    [now, settings.timeFormat]
  );

  return (
    <div className="ot-user-settings">
      <header className="ot-user-settings__head">
        <h2 className="ot-user-settings__title">User Settings</h2>
        <p className="ot-user-settings__subtitle">
          Choose how dates and times appear throughout OneTerminal.
        </p>
      </header>

      <SettingField
        id="date-format"
        label="Date format"
        value={settings.dateFormat}
        options={DATE_OPTIONS}
        onChange={(v) => saveSettings({ dateFormat: v })}
      />

      <SettingField
        id="time-format"
        label="Time format"
        value={settings.timeFormat}
        options={TIME_OPTIONS}
        onChange={(v) => saveSettings({ timeFormat: v })}
      />

      <SettingField
        id="first-day-of-week"
        label="First day of week"
        value={settings.firstDayOfWeek}
        options={FDOW_OPTIONS}
        onChange={(v) => saveSettings({ firstDayOfWeek: v })}
      />

      <section className="ot-user-settings__preview" aria-live="polite">
        <span className="ot-user-settings__preview-label">Preview</span>
        <span className="ot-user-settings__preview-line">
          <strong>{previewDate}</strong> · <strong>{previewTime}</strong>
        </span>
        <span className="ot-user-settings__preview-line ot-user-settings__preview-line--muted">
          Week starts on {dayName(settings.firstDayOfWeek)}
        </span>
      </section>
    </div>
  );
}

// ── Generic labelled select field ────────────────────────────────────────────

interface FieldProps<T extends string> {
  id: string;
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}

function SettingField<T extends string>({ id, label, value, options, onChange }: FieldProps<T>) {
  return (
    <label className="ot-user-settings__field" htmlFor={id}>
      <span className="ot-user-settings__field-label">{label}</span>
      <select
        id={id}
        className="ot-user-settings__select"
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
