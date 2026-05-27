/**
 * Reference-counted panel parking.
 *
 * Multiple UI surfaces (modal dialogs, the App Menu drawer, the engine
 * picker, the unsaved-changes confirm, etc) all need to park panel webviews
 * while open so clicks land on chrome instead of falling through to panels
 * sitting above in z-order.
 *
 * Without refcounting, callers race: surface A parks → surface B parks →
 * surface B closes and unparks → panels return while surface A still needs
 * them parked. This module centralises the bookkeeping so the Rust
 * `wm_unpark_panels` only fires when the *last* caller releases.
 *
 * Use `pushPark()` when opening, `popPark()` when closing. Pair them
 * carefully — a missing `popPark()` leaks panels offscreen.
 */

import { invoke } from "@tauri-apps/api/core";

let count = 0;

export function pushPark(): void {
  count++;
  if (count === 1) {
    invoke("wm_park_panels").catch(console.error);
  }
}

export function popPark(): void {
  if (count === 0) return; // defensive — extra pop shouldn't unpark
  count--;
  if (count === 0) {
    invoke("wm_unpark_panels").catch(console.error);
  }
}
