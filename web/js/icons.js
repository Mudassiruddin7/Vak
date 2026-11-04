// A small set of line icons drawn on one 24px grid with the same round-cap pen, so nothing relies on emoji.
import { raw } from "./dom.js";

const PATHS = {
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3"/>',
  speaker: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>',
  copy: '<rect x="8.5" y="8.5" width="11" height="11" rx="2"/><path d="M15.5 8.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7.5a2 2 0 0 0 2 2h2.5"/>',
  swap: '<path d="M4 8h14l-3.5-3.5M20 16H6l3.5 3.5"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  lock: '<rect x="5" y="10.5" width="14" height="9.5" rx="2"/><path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5"/>',
  arrow: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  play: '<path d="M8 5.5v13l10-6.5z"/>',
  pause: '<path d="M8.5 5.5v13M15.5 5.5v13"/>',
  stop: '<rect x="6.5" y="6.5" width="11" height="11" rx="1.5"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  down: '<path d="M6 9.5l6 6 6-6"/>',
  back: '<path d="M10 7l-5 5 5 5M5 12h14"/>',
};

export function icon(name, size = 20) {
  return raw(`<svg class="ic" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${PATHS[name] || ""}</svg>`);
}

// Proofreader's marks: a tick for an answer that held, a wave for one that wobbled, a strike for one that changed.
const MARKS = {
  ok: '<path d="M3 11.5l4.2 4.3L17 5"/>',
  wobble: '<path d="M2 11c2.2-4 3.8-4 6 0s3.8 4 6 0 3.8-4 6 0"/>',
  broke: '<path d="M3 17L17 3M3 3l14 14" />',
  error: '<path d="M3 10h14" stroke-dasharray="1 3.5"/>',
};

export function mark(status, size = 18) {
  const d = MARKS[status] || MARKS.error;
  return raw(`<svg class="mk ${status}" width="${size}" height="${size}" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`);
}

/** The Vak mark, the "bindu v": a lowercase v with one dot, like the dot that marks a nasal sound in Devanagari
 *  and Telugu. Same drawing as img/favicon.svg. */
export const LOGO = raw(`<svg class="logo-mark" width="32" height="32" viewBox="0 0 64 64" aria-hidden="true"><rect width="64" height="64" rx="18" fill="#3A5F7D"/><path d="M15 26 L28 48 L41 26" fill="none" stroke="#F9FBFC" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round"/><circle cx="47" cy="17" r="4.8" fill="#EBD3B8"/></svg>`);
