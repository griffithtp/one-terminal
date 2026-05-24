/**
 * Cached `wm_config` accessor.
 *
 * The Tauri `wm_config` invoke returns the terminal's static configuration
 * (title, app directory URL, etc). Multiple modules need it at mount —
 * Header for the brand label, useAppLaunch for the app directory URL — and
 * the config is immutable for the lifetime of the window. Cache the promise
 * so only one Rust call happens regardless of caller count.
 */

import { invoke } from "@tauri-apps/api/core";
import type { TerminalConfig } from "../types";

let cached: Promise<TerminalConfig> | null = null;

export function getTerminalConfig(): Promise<TerminalConfig> {
  if (!cached) cached = invoke<TerminalConfig>("wm_config");
  return cached;
}
