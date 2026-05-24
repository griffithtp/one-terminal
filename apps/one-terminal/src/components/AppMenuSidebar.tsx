/**
 * AppMenuSidebar
 *
 * Left-side drawer that hosts the Terminal app menu. Two-pane layout:
 * section list on the left rail, active section body on the right.
 *
 * Lifecycle:
 *   - Mounts on first open; stays mounted between opens so the slide-out
 *     animation can play on close.
 *   - Parks panel webviews while open (chrome z-order is below panel
 *     webviews — without parking, clicks fall through to whichever panel
 *     covers the drawer).
 *   - Escape and backdrop click both close.
 *
 * Section list is provided by the parent. Each section's `render()` runs
 * lazily — sections that haven't been visited never mount.
 */

import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./AppMenuSidebar.css";

export interface SectionDef {
  id: string;
  label: string;
  icon?: ReactNode;
  render: () => ReactNode;
}

interface Props {
  open: boolean;
  onClose: () => void;
  sections: SectionDef[];
  defaultSectionId?: string;
}

const ANIM_MS = 160;

export function AppMenuSidebar({ open, onClose, sections, defaultSectionId }: Props) {
  const [mounted, setMounted] = useState(false);
  const [entering, setEntering] = useState(false);
  const [activeId, setActiveId] = useState<string | undefined>(
    () => defaultSectionId ?? sections[0]?.id
  );

  // Mount/unmount with animation. On open: mount immediately, then flip the
  // `entering` flag on the next frame so the CSS transition fires. On close:
  // flip entering off and unmount after the transition completes.
  useEffect(() => {
    if (open) {
      setMounted(true);
      const raf = requestAnimationFrame(() => setEntering(true));
      return () => cancelAnimationFrame(raf);
    } else {
      setEntering(false);
      const t = setTimeout(() => setMounted(false), ANIM_MS);
      return () => clearTimeout(t);
    }
  }, [open]);

  // Park panels while the drawer is *mounted* (covers both entering and
  // exiting animations). The panel webviews sit above the chrome in z-order,
  // so without parking the user could click through the backdrop onto a
  // widget while the drawer is sliding out.
  const parkedRef = useRef(false);
  useEffect(() => {
    if (mounted && !parkedRef.current) {
      parkedRef.current = true;
      invoke("wm_park_panels").catch(console.error);
    } else if (!mounted && parkedRef.current) {
      parkedRef.current = false;
      invoke("wm_unpark_panels").catch(console.error);
    }
  }, [mounted]);

  // Escape to close
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  // If the section list changes such that activeId is no longer present,
  // fall back to the first section. Also re-apply the parent's default
  // when the drawer reopens (so deep-linking from outside works).
  useEffect(() => {
    if (open && defaultSectionId) {
      setActiveId(defaultSectionId);
    }
  }, [open, defaultSectionId]);

  useEffect(() => {
    if (activeId && !sections.find((s) => s.id === activeId)) {
      setActiveId(sections[0]?.id);
    }
  }, [sections, activeId]);

  const handleBackdropClick = useCallback(() => onClose(), [onClose]);

  if (!mounted) return null;

  const active = sections.find((s) => s.id === activeId);
  const openClass = entering ? " ot-menu--open" : "";

  return (
    <>
      <div
        className={`ot-menu-backdrop${openClass}`}
        onClick={handleBackdropClick}
        aria-hidden
      />
      <aside
        className={`ot-menu-panel${openClass}`}
        role="dialog"
        aria-modal="true"
        aria-label="App menu"
      >
        <header className="ot-menu-header">
          <span className="ot-menu-header__title">Menu</span>
          <button
            type="button"
            className="ot-menu-header__close"
            onClick={onClose}
            aria-label="Close menu"
          >
            ✕
          </button>
        </header>

        <div className="ot-menu-body">
          <nav className="ot-menu-nav" aria-label="Sections">
            {sections.map((s) => {
              const isActive = s.id === activeId;
              return (
                <button
                  key={s.id}
                  type="button"
                  className={`ot-menu-nav__item${isActive ? " ot-menu-nav__item--active" : ""}`}
                  onClick={() => setActiveId(s.id)}
                  aria-current={isActive ? "page" : undefined}
                >
                  {s.icon && (
                    <span className="ot-menu-nav__icon" aria-hidden>
                      {s.icon}
                    </span>
                  )}
                  <span className="ot-menu-nav__label">{s.label}</span>
                </button>
              );
            })}
          </nav>

          <div className="ot-menu-content">{active?.render()}</div>
        </div>
      </aside>
    </>
  );
}
