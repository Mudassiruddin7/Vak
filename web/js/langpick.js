// One language picker for the whole app: endonym first, English second, searchable in either script,
// grouped by script, with a note on how each language's wording was made. A plain list (not a <select>)
// so find-in-page and screen readers see every option.
import { html, mount, el, esc } from "./dom.js";
import { t } from "./i18n.js";

const GROUPS = [
  ["devanagari", "Devanagari"], ["bengali", "Bengali / Assamese"], ["telugu", "Dravidian"], ["tamil", "Dravidian"], ["kannada", "Dravidian"], ["malayalam", "Dravidian"],
  ["gujarati", "Gujarati"], ["gurmukhi", "Gurmukhi"], ["odia", "Odia"], ["arabic", "Perso-Arabic"], ["olchiki", "Ol Chiki"], ["meitei", "Meitei Mayek"], ["latin", "English"],
];
const ORDER = ["latin", "devanagari", "telugu", "tamil", "kannada", "malayalam", "bengali", "gujarati", "gurmukhi", "odia", "arabic", "meitei", "olchiki"];
const NAME = { latin: "English", devanagari: "Devanagari", telugu: "Telugu script", tamil: "Tamil script", kannada: "Kannada script", malayalam: "Malayalam script", bengali: "Bengali script", gujarati: "Gujarati script", gurmukhi: "Gurmukhi", odia: "Odia script", arabic: "Perso-Arabic", meitei: "Meitei Mayek", olchiki: "Ol Chiki" };
const ALIASES = { od: "oriya odia", mni: "meitei manipuri", brx: "boro bodo", kok: "konkani goan", ur: "urdu", pa: "punjabi", ks: "kashmiri koshur", sd: "sindhi", sat: "santali" };

export const quality = (l) => (l.quality === "handwritten" ? "" : l.quality === "limited" ? t("a.mtl") : t("a.mt"));

/** Opens the picker under `anchor`. `onPick(code)` fires with a language code, or "auto" when `auto` is set. */
export function pickLanguage({ anchor, languages, current, onPick, auto = false, recents = [], only = null }) {
  document.querySelector(".lang-menu")?.remove();
  const list = languages.filter((l) => !only || only.includes(l.code));
  const menu = el("div", { class: "lang-menu", role: "dialog", "aria-label": "Language" });
  const draw = (q = "") => {
    const needle = q.trim().toLowerCase();
    const match = (l) => !needle || `${l.native} ${l.english} ${l.code} ${ALIASES[l.code] || ""}`.toLowerCase().includes(needle);
    const rec = recents.map((c) => list.find((l) => l.code === c)).filter((l) => l && match(l));
    const byScript = ORDER.map((s) => [s, list.filter((l) => l.script === s && match(l))]).filter(([, ls]) => ls.length);
    const row = (l) => html`<button type="button" role="option" data-pick="${l.code}" aria-selected="${l.code === current}" lang="${l.code}" dir="${l.script === "arabic" ? "rtl" : "ltr"}"><span>${l.native}</span><small>${l.english !== l.native ? l.english : ""}${quality(l) ? ` · ${quality(l)}` : ""}</small></button>`;
    mount(menu.querySelector(".lm-list"), html`
      ${auto && !needle ? html`<button type="button" role="option" data-pick="auto" aria-selected="${current === "auto"}"><span>${t("tr.detect")}</span><small></small></button>` : ""}
      ${rec.length ? html`<p class="lm-h">${t("tr.recent")}</p>${rec.map(row)}` : ""}
      ${byScript.map(([s, ls]) => html`<p class="lm-h">${NAME[s]}</p>${ls.map(row)}`)}
      ${!rec.length && !byScript.length ? html`<p class="hint" style="padding:.6rem .8rem">—</p>` : ""}`);
  };
  mount(menu, html`<input class="input lm-search" type="search" placeholder="${t("tr.search")}" aria-label="${t("tr.search")}" autocomplete="off" spellcheck="false"><div class="lm-list" role="listbox"></div>`);
  draw();
  const r = anchor.getBoundingClientRect();
  menu.style.top = `${r.bottom + window.scrollY + 6}px`;
  const rtl = document.documentElement.dir === "rtl";
  if (rtl) menu.style.right = `${Math.max(8, document.documentElement.clientWidth - r.right)}px`; else menu.style.left = `${Math.max(8, Math.min(r.left, document.documentElement.clientWidth - 330))}px`;
  document.body.append(menu);
  const close = () => { menu.remove(); document.removeEventListener("pointerdown", away, true); document.removeEventListener("keydown", esc); };
  const away = (e) => { if (!menu.contains(e.target) && !anchor.contains(e.target)) close(); };
  const esc = (e) => { if (e.key === "Escape") { close(); anchor.focus(); } };
  setTimeout(() => { document.addEventListener("pointerdown", away, true); document.addEventListener("keydown", esc); }, 0);
  const search = menu.querySelector(".lm-search");
  search.addEventListener("input", () => draw(search.value));
  menu.addEventListener("click", (e) => { const b = e.target.closest("[data-pick]"); if (b) { close(); onPick(b.dataset.pick); } });
  search.focus();
  menu.querySelector("[aria-selected=true]")?.scrollIntoView({ block: "nearest" });
}
