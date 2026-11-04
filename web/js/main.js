import { $, html, mount, el } from "./dom.js";
import { t, setLang, getLang, initialLang, loadLang } from "./i18n.js";
import "./strings.js";
import * as S from "./state.js";
import * as auth from "./auth.js";
import { api } from "./api.js";
import { LOGO, icon } from "./icons.js";
import { toast, reveal } from "./fx.js";
import { pickLanguage } from "./langpick.js";
import { home } from "./views/home.js";
import { setu } from "./views/setu.js";
import { learn } from "./views/learn.js";
import { rules } from "./views/rules.js";
import { translate } from "./views/translate.js";

export const app = { config: null, route: "audit", cleanup: null, uiLangs: ["en", "hi", "te"] };
const ROUTES = { "": ["audit", home], "/": ["audit", home], "/translate": ["translate", translate], "/setu": ["setu", setu], "/learn": ["learn", learn], "/rules": ["rules", rules] };
const NAV = [["audit", "#/"], ["translate", "#/translate"], ["setu", "#/setu"], ["learn", "#/learn"], ["rules", "#/rules"]];

function applyTheme() {
  document.documentElement.dataset.theme = "light"; // one white day theme, on purpose
}

const native = (code) => app.config.languages.find((l) => l.code === code)?.native || code.toUpperCase();

async function pickLang(code) {
  await loadLang(code);
  setLang(code); S.set({ uiLang: code }); renderChrome(); route();
}

function renderChrome() {
  mount($("#nav"), html`
    <a class="logo" href="#/" aria-label="Vak">${LOGO}<span>vak</span><small>వాక్కు</small></a>
    <nav class="nav" aria-label="Sections">${NAV.map(([id, href]) => html`<a href="${href}" ${app.route === id ? 'aria-current="page"' : ""}>${t("nav." + id)}</a>`)}</nav>
    <div class="hdr-end">
      <button class="link" type="button" id="lang-btn" aria-haspopup="dialog" lang="${getLang()}">${native(getLang())}</button>
      <button class="link" type="button" id="open-settings">${auth.user() ? auth.user().email?.split("@")[0] : t("nav.settings")}</button>
    </div>`);
  $("#lang-btn").addEventListener("click", (e) => pickLanguage({ anchor: e.currentTarget, languages: app.config.languages, only: app.uiLangs, current: getLang(), onPick: pickLang }));
  $("#open-settings").addEventListener("click", openSettings);
}

export function route() {
  const path = location.hash.replace(/^#/, "");
  const [name, view] = ROUTES[path] || ROUTES[""];
  app.route = name;
  app.cleanup?.();
  renderChrome();
  const host = $("#app");
  host.replaceChildren();
  app.cleanup = view(host, app) || null;
  reveal(host);
  window.scrollTo({ top: 0 });
  document.title = `Vak | ${t("nav." + name)}`;
}

function openSettings() {
  const s = S.get();
  const back = el("div", { class: "modal-back" });
  const close = () => back.remove();
  back.addEventListener("click", (e) => { if (e.target === back) close(); });
  const signedIn = !!auth.user();
  mount(back.appendChild(el("div", { class: "modal", role: "dialog", "aria-modal": "true" })), html`
    <div class="bench-head"><h3 class="h3">${t("st.title")}</h3><button class="icon-btn" type="button" id="x" aria-label="${t("st.close")}">${icon("close", 18)}</button></div>
    <div class="stack">
      <p class="muted">${t("st.left")}: <b id="left">…</b></p>
      <label class="field">${t("st.key")}<input class="input" id="key" type="password" autocomplete="off" spellcheck="false" placeholder="sk_…" value="${s.ownKey}"></label>
      <p class="hint">${t("st.keyd")}</p>
      <div class="row"><button class="btn sm" id="save-key" type="button">${t("st.save")}</button><button class="btn line" id="rm-key" type="button">${t("st.rm")}</button></div>
      <hr style="border:0;border-top:1px solid var(--rule);width:100%">
      ${!auth.enabled() ? html`<p class="hint">Sign-in isn’t set up on this server.</p>` : signedIn
        ? html`<p>${t("au.hi")} <b>${auth.user().email}</b></p><button class="btn line" id="out" type="button">${t("nav.signout")}</button>`
        : html`<p class="muted">${t("au.note")}</p>
          <label class="field">${t("au.email")}<input class="input" id="em" type="email" autocomplete="email"></label>
          <label class="field">${t("au.pass")}<input class="input" id="pw" type="password" autocomplete="current-password" minlength="8"></label>
          <div class="row"><button class="btn sm" id="in" type="button">${t("au.in")}</button><button class="btn line" id="up" type="button">${t("au.up")}</button></div>
          <p class="hint" id="auth-msg" role="status"></p>`}
    </div>`);
  document.body.append(back);
  const q = (sel) => back.querySelector(sel);
  q("#x").addEventListener("click", close);
  api("/api/config").then((c) => { q("#left").textContent = Math.round(c.quota_left * 10) / 10; }).catch(() => {});
  q("#save-key").addEventListener("click", () => { S.set({ ownKey: q("#key").value.trim() }); toast("OK"); });
  q("#rm-key").addEventListener("click", () => { S.set({ ownKey: "" }); q("#key").value = ""; });
  q("#out")?.addEventListener("click", () => { auth.signOut(); close(); renderChrome(); });
  const go = (fn) => async () => {
    const msg = q("#auth-msg");
    try { const r = await fn(q("#em").value.trim(), q("#pw").value); if (r === "confirm") { msg.textContent = t("au.confirm"); return; } close(); renderChrome(); route(); }
    catch (e) { msg.textContent = e.message; }
  };
  q("#in")?.addEventListener("click", go((e, p) => auth.signIn(e, p)));
  q("#up")?.addEventListener("click", go((e, p) => auth.signUp(e, p)));
}

async function boot() {
  applyTheme();
  try { app.config = await api("/api/config"); if (app.config.supabase) auth.configure(app.config.supabase); }
  catch { app.config = { languages: [], efforts: [], domains: [], models: [], practice: [], limits: {} }; toast(t("err.network")); }
  let extra = [];
  try { extra = await (await fetch("/i18n/index.json")).json(); } catch { /* only the built-in three */ }
  const known = new Set(app.config.languages.map((l) => l.code));
  app.uiLangs = ["en", "hi", "te", ...extra].filter((c, i, a) => a.indexOf(c) === i && (known.has(c) || ["en", "hi", "te"].includes(c)));
  const want = S.get().uiLang || initialLang();
  await loadLang(want); setLang(want);
  window.addEventListener("hashchange", route);
  route();
}
boot();
