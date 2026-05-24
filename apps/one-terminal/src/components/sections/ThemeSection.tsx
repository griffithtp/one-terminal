/**
 * ThemeSection
 *
 * Light / Dark / System chooser, intended to be rendered inside the App Menu
 * drawer (Plan 10-C). Owns no modal shell — the parent provides the panel.
 *
 * Persists via theme/themeStore and applies immediately so the user sees the
 * change reflect across the chrome before they leave the section.
 */

import { useCallback, useEffect, useState } from "react";
import {
  applyTheme,
  loadTheme,
  saveTheme,
  subscribeEffectiveTheme,
  type EffectiveTheme,
  type ThemeChoice,
} from "../../theme/themeStore";
import "./ThemeSection.css";

const OPTIONS: { value: ThemeChoice; label: string; hint: string }[] = [
  { value: "light", label: "Light", hint: "Bright palette" },
  { value: "dark", label: "Dark", hint: "Default OneTerminal palette" },
  { value: "system", label: "System", hint: "Follow OS appearance" },
];

function readEffective(): EffectiveTheme {
  const t = document.documentElement.dataset.theme;
  return t === "light" ? "light" : "dark";
}

export function ThemeSection() {
  const [choice, setChoice] = useState<ThemeChoice>(() => loadTheme());
  const [effective, setEffective] = useState<EffectiveTheme>(() => readEffective());

  useEffect(() => subscribeEffectiveTheme(setEffective), []);

  const handleChange = useCallback((next: ThemeChoice) => {
    setChoice(next);
    saveTheme(next);
    applyTheme(next);
  }, []);

  return (
    <div className="ot-theme-section">
      <header className="ot-theme-section__head">
        <h2 className="ot-theme-section__title">Appearance</h2>
        <p className="ot-theme-section__subtitle">
          Current: <strong>{effective === "dark" ? "Dark" : "Light"}</strong>
          {choice === "system" && " (following system)"}
        </p>
      </header>

      <ul className="ot-theme-section__list" role="radiogroup" aria-label="Theme">
        {OPTIONS.map((opt) => {
          const selected = choice === opt.value;
          return (
            <li key={opt.value}>
              <label
                className={`ot-theme-option${selected ? " ot-theme-option--selected" : ""}`}
              >
                <input
                  type="radio"
                  name="ot-theme"
                  value={opt.value}
                  checked={selected}
                  onChange={() => handleChange(opt.value)}
                  className="ot-theme-option__radio"
                />
                <span className="ot-theme-option__body">
                  <span className="ot-theme-option__label">{opt.label}</span>
                  <span className="ot-theme-option__hint">{opt.hint}</span>
                </span>
                <span className="ot-theme-option__check" aria-hidden>
                  {selected ? "●" : "○"}
                </span>
              </label>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
