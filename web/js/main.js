import { $, html, mount, el } from "./dom.js";
import { t, setLang, getLang, initialLang } from "./i18n.js";
import "./strings.js";
import * as S from "./state.js";
import * as auth from "./auth.js";
import { api } from "./api.js";
import { MITTHU_MARK } from "./mascot-svg.js";
import { toast, reveal } from "./fx.js";
import { home } from "./views/home.js";
import { setu } from "./views/setu.js";
import { learn } from "./views/learn.js";
import { rules } from "./views/rules.js";

export const app = { config: null, route: "audit", cleanup: null };
const ROUTES = { "": ["audit", home], "/": ["audit", home], "/setu": ["setu", setu], "/learn": ["learn", learn], "/rules": ["rules", rules] };

function applyTheme() {
  const th = S.get().theme;
  if (th) document.documentElement.dataset.theme = th; else delete document.documentElement.dataset.theme;
}

function renderChrome() {
  const lang = getLang();
  mount($("#announce"), html`${t("announce.text")} <a href="#/setu">${t("announce.cta")} →</a>`);
  const s = S.get();
  mount($("#nav"), html`
    <a class="logo" href="#/" aria-label="Vak"><span class="bars" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>Vak</a>
    <nav class="seg" aria-label="Sections">
      <a href="#/" class="${app.route === "audit" ? "on" : ""}">${t("nav.audit")}</a>
      <a href="#/setu" class="${app.route === "setu" ? "on" : ""}">${t("nav.setu")}</a>
      <a href="#/learn" class="${app.route === "learn" ? "on" : ""}">${t("nav.learn")}</a>
    </nav>
    <div class="nav-links">
      <a class="hide-sm" href="#/rules">${t("nav.rules")}</a>
      <span class="langpick" role="group" aria-label="Language">
        ${["te", "hi", "en"].map((c) => html`<button type="button" data-lang="${c}" aria-pressed="${lang === c}">${c === "te" ? "తె" : c === "hi" ? "हि" : "EN"}</button>`)}
      </span>
      <button class="link" type="button" id="open-settings">${auth.user() ? auth.user().email?.split("@")[0] : t("nav.settings")}</button>
    </div>`);
  $("#nav").querySelectorAll("[data-lang]").forEach((b) => b.addEventListener("click", () => { setLang(b.dataset.lang); S.set({ uiLang: b.dataset.lang }); renderChrome(); route(); }));
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
    <div class="bench-head"><h3 class="h3">${t("st.title")}</h3><button class="icon-btn" type="button" id="x" aria-label="${t("st.close")}">✕</button></div>
    <div class="stack">
      <p class="muted">${t("st.left")}: <b id="left">…</b></p>
      <label class="field">${t("st.key")}<input class="input" id="key" type="password" autocomplete="off" spellcheck="false" placeholder="sk_…" value="${s.ownKey}"></label>
      <p class="hint">${t("st.keyd")}</p>
      <div class="row"><button class="btn sm" id="save-key" type="button">${t("st.save")}</button><button class="btn sm line" id="rm-key" type="button">${t("st.rm")}</button></div>
      <div class="row"><span class="muted">${t("st.theme")}</span>
        <span class="seg" role="group">${["", "light", "dark"].map((v) => html`<button type="button" data-theme-pick="${v}" class="${s.theme === v ? "on" : ""}">${v || "Auto"}</button>`)}</span></div>
      <hr style="border:0;border-top:1.5px solid var(--line-soft);width:100%">
      ${!auth.enabled() ? html`<p class="hint">Sign-in isn’t set up on this server.</p>` : signedIn
        ? html`<p>${t("au.hi")} <b>${auth.user().email}</b></p><button class="btn sm line" id="out" type="button">${t("nav.signout")}</button>`
        : html`<p class="muted">${t("au.note")}</p>
          <label class="field">${t("au.email")}<input class="input" id="em" type="email" autocomplete="email"></label>
          <label class="field">${t("au.pass")}<input class="input" id="pw" type="password" autocomplete="current-password" minlength="8"></label>
          <div class="row"><button class="btn sm" id="in" type="button">${t("au.in")}</button><button class="btn sm line" id="up" type="button">${t("au.up")}</button></div>
          <p class="hint" id="auth-msg" role="status"></p>`}
    </div>`);
  document.body.append(back);
  const q = (s) => back.querySelector(s);
  q("#x").addEventListener("click", close);
  api("/api/config").then((c) => { q("#left").textContent = Math.round(c.quota_left * 10) / 10; }).catch(() => {});
  q("#save-key").addEventListener("click", () => { S.set({ ownKey: q("#key").value.trim() }); toast("OK"); });
  q("#rm-key").addEventListener("click", () => { S.set({ ownKey: "" }); q("#key").value = ""; });
  back.querySelectorAll("[data-theme-pick]").forEach((b) => b.addEventListener("click", () => { S.set({ theme: b.dataset.themePick }); applyTheme(); close(); }));
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
  setLang(S.get().uiLang || initialLang());
  applyTheme();
  try { app.config = await api("/api/config"); if (app.config.supabase) auth.configure(app.config.supabase); }
  catch { app.config = { languages: [], efforts: [], domains: [], models: [], practice: [], limits: {} }; toast(t("err.network")); }
  window.addEventListener("hashchange", route);
  route();
}
export { MITTHU_MARK };
boot();
