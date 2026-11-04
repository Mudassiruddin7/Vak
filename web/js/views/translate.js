// A text translator for the 23 languages Sarvam covers. It translates as you pause and keeps the previous
// answer on screen until the next one arrives. It also handles what people in India actually do: typing in
// English letters, choosing a register, native or international digits, speaking, and hearing it back.
import { html, mount, $, $$, debounce } from "../dom.js";
import { t } from "../i18n.js";
import { api, speak, ApiError } from "../api.js";
import * as S from "../state.js";
import { startRecording, micSupported } from "../recorder.js";
import { toast } from "../fx.js";
import { icon } from "../icons.js";
import { pickLanguage, quality } from "../langpick.js";

const MAX = 2000;
const msg = (e) => (e instanceof ApiError ? (e.message === "network" ? t("err.network") : e.message) : e.message === "silent" ? t("err.mic") : e.message || t("err.generic"));
/** Direction follows the script of the text itself, since the same language can be written in two scripts. */
const dirOf = (text) => ((text.match(/[؀-ۿݐ-ݿ]/g) || []).length > (text.match(/\p{L}/gu) || []).length / 2 ? "rtl" : "ltr");

export function translate(host, app) {
  const langs = app.config.languages;
  const L = (c) => langs.find((l) => l.code === c);
  const saved = S.get().tr || {};
  const st = { src: saved.src || "auto", dst: saved.dst || (document.documentElement.lang === "hi" ? "en" : "hi"), recents: saved.recents || ["te", "hi", "en"], text: "", out: null, detected: "", roman: false, native: false, gender: "", style: "formal", back: "", busy: false, rec: null };
  let abort = null, audio = null;
  const keep = () => S.set({ tr: { src: st.src, dst: st.dst, recents: st.recents.slice(0, 5) } });
  const bump = (c) => { st.recents = [c, ...st.recents.filter((x) => x !== c)].slice(0, 5); };
  const label = (c) => (c === "auto" ? `${t("tr.detect")}${st.detected ? ` · ${L(st.detected)?.native || ""}` : ""}` : L(c)?.native || c);

  function draw() {
    const shown = st.out ? (st.style === "casual" && st.out.alternative ? st.out.alternative : st.out.translation) : "";
    const dl = L(st.dst), srcLang = st.src === "auto" ? st.detected || "en" : st.src;
    mount(host, html`<section class="sheet" style="padding-top:1.6rem"><div class="bench" style="width:min(100% - 2 * var(--gutter), 64rem)">
      <div class="tr-top"><span>${t("tr.from")}</span><button class="input" type="button" id="pick-src" style="text-align:start;cursor:pointer" lang="${srcLang}">${label(st.src)}</button>
        <button class="icon-btn" type="button" id="swap" title="${t("tr.swap")} (Ctrl+Shift+S)" aria-label="${t("tr.swap")}">${icon("swap", 18)}</button>
        <span>${t("tr.to")}</span><button class="input" type="button" id="pick-dst" style="text-align:start;cursor:pointer" lang="${st.dst}">${label(st.dst)}</button></div>
      ${dl && quality(dl) ? html`<p class="hint" style="margin:-.6rem 0 .8rem">${quality(dl)}</p>` : ""}
      <div class="tr-grid">
        <div class="tr-pane">
          <textarea class="tr-text" id="in" placeholder="${t("tr.ph")}" maxlength="${MAX}" lang="${srcLang}" dir="auto">${st.text}</textarea>
          <div class="tr-foot"><span class="hint" id="count">${st.text.length} / ${MAX}</span>
            <label class="chip" style="cursor:pointer"><input type="checkbox" id="roman" ${st.roman ? "checked" : ""}> ${t("tr.roman")}</label>
            ${micSupported() ? html`<button class="mic ${st.rec ? "rec" : ""}" style="width:2.6rem;height:2.6rem" type="button" id="mic" aria-label="${t("tr.mic")}">${icon(st.rec ? "stop" : "mic", 18)}</button>` : ""}</div>
        </div>
        <div class="tr-pane">
          <div class="tr-bar ${st.busy ? "on" : ""}"></div>
          <div class="tr-text" id="out" lang="${st.dst}" dir="${shown ? dirOf(shown) : "auto"}">${shown || html`<span class="hint">${t("tr.empty")}</span>`}</div>
          <div class="tr-foot">
            ${st.out?.alternative ? html`<span class="seg" role="group"><button type="button" data-style="formal" class="${st.style === "formal" ? "on" : ""}">${t("tr.formal")}</button><button type="button" data-style="casual" class="${st.style === "casual" ? "on" : ""}">${t("tr.casual")}</button></span>` : ""}
            <select class="input" id="gender" style="width:auto;font-size:.9rem" aria-label="${t("tr.gender")}"><option value="">${t("tr.gender")}: —</option><option value="Female" ${st.gender === "Female" ? "selected" : ""}>${t("tr.gender")}: ♀</option><option value="Male" ${st.gender === "Male" ? "selected" : ""}>${t("tr.gender")}: ♂</option></select>
            <label class="chip" style="cursor:pointer"><input type="checkbox" id="native" ${st.native ? "checked" : ""}> ${t("tr.numerals")}</label>
            <span class="grow"></span>
            ${shown && dl?.voice ? html`<button class="btn line" type="button" id="say">${icon("speaker", 16)} ${t("tr.listen")}</button>` : ""}
            ${shown ? html`<button class="btn line" type="button" id="copy">${icon("copy", 16)} ${t("tr.copy")}</button>` : ""}</div>
          ${shown ? html`<div class="tr-back"><button class="btn line" type="button" id="back">${t("tr.back")}</button>${st.back ? html`<p class="muted" lang="${srcLang}" dir="auto" style="margin-top:.5rem">${st.back}</p>` : ""}</div>` : ""}
          ${st.out?.same ? html`<p class="hint">${t("tr.same")}</p>` : ""}
        </div>
      </div></div></section>`);
    wire();
  }

  async function run() {
    const text = st.text.trim();
    if (!text) { st.out = null; st.detected = ""; st.back = ""; return draw(); }
    abort?.abort(); abort = new AbortController(); st.busy = true; $(".tr-bar", host)?.classList.add("on");
    try {
      const r = await api("/api/translate", { method: "POST", signal: abort.signal, body: { text, src: st.src, dst: st.dst, romanized: st.roman, native_numerals: st.native, gender: st.gender || undefined, alternatives: true } });
      st.out = r; st.detected = r.detected; st.back = ""; if (!r.alternative) st.style = "formal";
      st.busy = false; const ta = $("#in", host), sel = ta?.selectionStart; draw(); const n = $("#in", host); n.focus(); if (sel != null) n.setSelectionRange(sel, sel);
    } catch (e) { if (e.name === "AbortError") return; st.busy = false; $(".tr-bar", host)?.classList.remove("on"); toast(msg(e)); }
  }
  const later = debounce(run, 450);

  function setLang(side, code) { st[side] = code; if (code !== "auto") bump(code); keep(); st.out = null; draw(); run(); }

  function wire() {
    const ta = $("#in", host);
    ta.addEventListener("input", () => { st.text = ta.value; $("#count", host).textContent = `${ta.value.length} / ${MAX}`; later(); });
    $("#pick-src", host).addEventListener("click", (e) => pickLanguage({ anchor: e.currentTarget, languages: langs, current: st.src, auto: true, recents: st.recents, onPick: (c) => setLang("src", c) }));
    $("#pick-dst", host).addEventListener("click", (e) => pickLanguage({ anchor: e.currentTarget, languages: langs, current: st.dst, recents: st.recents, onPick: (c) => setLang("dst", c) }));
    $("#swap", host).addEventListener("click", swap);
    $$("[data-style]", host).forEach((b) => b.addEventListener("click", () => { st.style = b.dataset.style; draw(); }));
    $("#roman", host).addEventListener("change", (e) => { st.roman = e.target.checked; run(); });
    $("#native", host).addEventListener("change", (e) => { st.native = e.target.checked; run(); });
    $("#gender", host).addEventListener("change", (e) => { st.gender = e.target.value; run(); });
    $("#copy", host)?.addEventListener("click", async () => { try { await navigator.clipboard.writeText($("#out", host).textContent); toast(t("tr.copied"), 1500); } catch { /* clipboard blocked */ } });
    $("#say", host)?.addEventListener("click", async () => { try { audio?.pause(); audio = new Audio(await speak($("#out", host).textContent, st.dst, (st.gender || "Female").toLowerCase())); await audio.play(); } catch (e) { toast(msg(e)); } });
    $("#back", host)?.addEventListener("click", async () => {
      try { const r = await api("/api/translate", { method: "POST", body: { text: $("#out", host).textContent, src: st.dst, dst: st.detected && st.detected !== st.dst ? st.detected : "en" } }); st.back = r.translation; draw(); } catch (e) { toast(msg(e)); }
    });
    $("#mic", host)?.addEventListener("click", mic);
  }

  function swap() {
    const from = st.src === "auto" ? st.detected || "en" : st.src;
    const text = st.out && $("#out", host) ? $("#out", host).textContent : st.text;
    st.src = st.dst; st.dst = from; st.text = text; bump(st.dst); keep(); st.out = null; draw(); run();
  }

  async function mic() {
    if (st.rec) {
      const r = st.rec; st.rec = null;
      try { const { blob } = await r.stop(); const f = new FormData(); f.append("audio", blob, "clip.wav"); f.append("lang", st.src === "auto" ? "auto" : st.src); const o = await api("/api/transcribe", { method: "POST", form: f }); st.text = o.text; if (st.src === "auto" && o.lang) st.detected = o.lang; draw(); run(); }
      catch (e) { draw(); toast(msg(e)); }
      return;
    }
    try { st.rec = await startRecording({ maxSeconds: 29, onAutoStop: mic }); draw(); } catch { toast(t("err.mic")); }
  }

  const key = (e) => { if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "s") { e.preventDefault(); swap(); } };
  window.addEventListener("keydown", key);
  draw();
  return () => { window.removeEventListener("keydown", key); abort?.abort(); audio?.pause(); st.rec?.cancel(); };
}
