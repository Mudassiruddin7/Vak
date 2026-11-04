// Stroke icons on a 24 x 24 grid, drawn by hand. `icon("mic")` returns safe markup.
import { raw } from "./dom.js";

const PATHS = {
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>',
  stop: '<rect x="7" y="7" width="10" height="10" rx="2.2" fill="currentColor" stroke="none"/>',
  go: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  compass: '<circle cx="12" cy="12" r="9"/><path d="M15.5 8.5l-2 5-5 2 2-5z"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>',
  chat: '<path d="M5 5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-7l-5 4v-4H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z"/>',
  volume: '<path d="M4 10v4h4l5 4V6L8 10z"/><path d="M16.5 9a4 4 0 0 1 0 6M19 6.5a8 8 0 0 1 0 11"/>',
  coin: '<circle cx="12" cy="12" r="9"/><path d="M9 8h6M9 11.2h6M9 8c4.2 0 4.2 5.2 0 5.2l4.2 3.8"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  moon: '<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>',
  resample: '<path d="M17 2.5l3.5 3.5-3.5 3.5"/><path d="M3.5 11V9.5A3.5 3.5 0 0 1 7 6h13.5M7 21.5L3.5 18 7 14.5"/><path d="M20.5 13v1.5A3.5 3.5 0 0 1 17 18H3.5"/>',
  typo: '<path d="M4 7V5h16v2M9 19h6M12 5v14"/>',
  format: '<path d="M4 7h16M4 12h10M4 17h16"/>',
  numerals: '<path d="M5 9h14M5 15h14M10.5 4L8.5 20M15.5 4l-2 16"/>',
  script: '<rect x="3" y="6" width="18" height="12" rx="2.5"/><path d="M7 10h.01M11 10h.01M15 10h.01M7 14h10"/>',
  paraphrase: '<path d="M16 3.5h4.5V8M3.5 20.5L20.5 3.5M20.5 16v4.5H16M15 15l5.5 5.5M3.5 3.5L9 9"/>',
  language: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  distractor: '<path d="M3 8h11a3 3 0 1 0-3-3M3 12h16a3 3 0 1 1-3 3M3 16h7"/>',
  polite: '<path d="M12 20s-7-4.5-7-10a4 4 0 0 1 7-2.5A4 4 0 0 1 19 10c0 5.5-7 10-7 10z"/>',
  negation: '<path d="M7 4L3 8l4 4M3 8h14a4 4 0 0 1 4 4M17 20l4-4-4-4M21 16H7a4 4 0 0 1-4-4"/>',
  control: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r=".9" fill="currentColor"/>',
  play: '<path d="M8 5.5v13l11-6.5z" fill="currentColor" stroke="none"/>',
  pause: '<rect x="6.5" y="5" width="4" height="14" rx="1.3" fill="currentColor" stroke="none"/><rect x="13.5" y="5" width="4" height="14" rx="1.3" fill="currentColor" stroke="none"/>',
  download: '<path d="M12 3.5v12M7 10.5l5 5 5-5M4.5 20h15"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2.2"/><path d="M16 8V6.2A2.2 2.2 0 0 0 13.8 4H6.2A2.2 2.2 0 0 0 4 6.2v7.6A2.2 2.2 0 0 0 6.2 16H8"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"/>',
  loader: '<path d="M12 3a9 9 0 1 0 9 9"/>',
  back: '<path d="M19 12H5M11 6l-6 6 6 6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
};

export function icon(name, extra = "") {
  const body = PATHS[name] ?? PATHS.search;
  return raw(
    `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" ${extra}>${body}</svg>`,
  );
}

// the test categories the engine reports, mapped to an icon
export const CATEGORY_ICON = {
  typo: "typo", format: "format", numerals: "numerals", script: "script", paraphrase: "paraphrase",
  language: "language", distractor: "distractor", polite: "polite", negation: "negation",
  control: "control", resample: "resample",
};
