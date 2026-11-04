// A text translator for the 23 languages Sarvam covers: translates as you pause, keeps the last answer on
// screen while the next one loads, and handles what Indian users actually do (typing in English letters,
// choosing a register, mixing English words, native or international digits).
import { html, mount, $, $$, debounce } from "../dom.js";
import { t } from "../i18n.js";
import { api, speak, ApiError } from "../api.js";
import * as S from "../state.js";
import { startRecording, micSupported } from "../recorder.js";
import { toast } from "../fx.js";

const MAX = 2000;
const msg = (e) => (e instanceof ApiError ? (e.message === "network" ? t("err.network") : e.message) : e.message === "silent" ? t("err.mic") : e.message || t("err.generic"));

export function translate(host, app) {
  const langs = app.config.languages;
  const L = (c) => langs.find((l) => l.code === c);
  const saved = S.get().tr || {};
  const st = { src: saved.src || "auto", dst: saved.dst || (document.documentElement.lang === "hi" ? "en" : "hi"), recents: saved.recents || ["te", "hi", "en"], text: "", out: null, detected: "", roman: false, native: false, gender: "", style: "formal", back: "", busy: false, rec: null };
  let abort = null, audio = null;
  const keep = () => S.set({ tr: { src: st.src, dst: st.dst, recents: st.recents.slice(0, 6) } });
  const name = (c) => (L(c) ? L(c).native : c);
  const bump = (c) => { st.recents = [c, ...st.recents.filter((x) => x !== c)].slice(0, 6); };

  function picker(side) {
    const cur = side === "src" ? st.src : st.dst;
    const rec = st.recents.filter((c) => c !== (side === "src" ? st.dst : st.src)).slice(0, 3);
    return html`<div class="tr-pick"><div class="chips">
      ${side === "src" ? html`<button class="chip ${cur === "auto" ? "on" : ""}" type="button" data-set="src:auto">${t("tr.detect")}</button>` : ""}
      ${rec.map((c) => html`<button class="chip ${cur === c ? "on" : ""}" type="button" data-set="${side}:${c}" lang="${c}">${name(c)}</button>`)}</div>
      <select class="input" data-all="${side}" aria-label="${side}">${side === "src" ? html`<option value="auto" ${cur === "auto" ? "selected" : ""}>${t("tr.detect")}</option>` : ""}${langs.map((l) => html`<option value="${l.code}" ${l.code === cur ? "selected" : ""}>${l.native}${l.native !== l.english ? ` · ${l.english}` : ""}</option>`)}</select></div>`;
  }

  function draw() {
    const shown = st.out ? (st.style === "casual" && st.out.alternative ? st.out.alternative : st.out.translation) : "";
    const dl = L(st.dst), srcShown = st.src === "auto" ? st.detected : st.src;
    mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench">
      <div class="tr-top"><h2 style="font-size:clamp(1.6rem,3vw,2.2rem)">${t("nav.translate")}</h2><button class="btn sm line" type="button" id="swap" title="Ctrl+Shift+S">⇄ ${t("tr.swap")}</button></div>
      <div class="tr-grid">
        <div class="tr-pane">
          <div class="tr-head">${picker("src")}${st.src === "auto" && st.detected ? html`<span class="hint">${t("tr.detected")}: ${name(st.detected)}</span>` : ""}</div>
          <textarea class="input tr-text" id="in" placeholder="${t("tr.ph")}" maxlength="${MAX}" lang="${srcShown || "en"}">${st.text}</textarea>
          <div class="tr-foot"><span class="hint" id="count">${st.text.length} / ${MAX}</span><span class="grow"></span>
            <label class="chip" style="cursor:pointer"><input type="checkbox" id="roman" ${st.roman ? "checked" : ""}> ${t("tr.roman")}</label>
            ${micSupported() ? html`<button class="icon-btn" type="button" id="mic" aria-label="${t("tr.mic")}" title="${t("tr.mic")}">${st.rec ? "■" : "🎙"}</button>` : ""}</div>
        </div>
        <div class="tr-pane out">
          <div class="tr-head">${picker("dst")}</div>
          <div class="tr-bar ${st.busy ? "on" : ""}"></div>
          <div class="tr-text say" id="out" lang="${st.dst}">${shown || html`<span class="hint">${t("tr.empty")}</span>`}</div>
          <div class="tr-foot">
            ${st.out?.alternative ? html`<span class="seg" role="group"><button type="button" data-style="formal" class="${st.style === "formal" ? "on" : ""}">${t("tr.formal")}</button><button type="button" data-style="casual" class="${st.style === "casual" ? "on" : ""}">${t("tr.casual")}</button></span>` : ""}
            <span class="grow"></span>
            <select class="input" id="gender" style="width:auto;font-size:.85rem" aria-label="${t("tr.gender")}"><option value="">${t("tr.gender")}: —</option><option value="Female" ${st.gender === "Female" ? "selected" : ""}>${t("tr.gender")}: ♀</option><option value="Male" ${st.gender === "Male" ? "selected" : ""}>${t("tr.gender")}: ♂</option></select>
            <label class="chip" style="cursor:pointer"><input type="checkbox" id="native" ${st.native ? "checked" : ""}> ${t("tr.numerals")}</label>
            ${shown && dl?.voice ? html`<button class="icon-btn" type="button" id="say" title="${t("tr.listen")}" aria-label="${t("tr.listen")}">🔊</button>` : ""}
            ${shown ? html`<button class="icon-btn" type="button" id="copy" title="${t("tr.copy")}" aria-label="${t("tr.copy")}">⧉</button>` : ""}</div>
          ${shown ? html`<div class="tr-back"><button class="btn sm line" type="button" id="back">${t("tr.back")}</button>${st.back ? html`<p class="muted" lang="${srcShown || "en"}" style="margin-top:.5rem">${st.back}</p>` : ""}</div>` : ""}
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

  function wire() {
    const ta = $("#in", host);
    ta.addEventListener("input", () => { st.text = ta.value; $("#count", host).textContent = `${ta.value.length} / ${MAX}`; later(); });
    $$("[data-set]", host).forEach((b) => b.addEventListener("click", () => { const [side, c] = b.dataset.set.split(":"); st[side] = c; if (side === "dst") bump(c); keep(); st.out = null; draw(); run(); }));
    $$("[data-all]", host).forEach((s) => s.addEventListener("change", () => { st[s.dataset.all] = s.value; if (s.value !== "auto") bump(s.value); keep(); st.out = null; draw(); run(); }));
    $("#swap", host).addEventListener("click", swap);
    $$("[data-style]", host).forEach((b) => b.addEventListener("click", () => { st.style = b.dataset.style; draw(); }));
    $("#roman", host).addEventListener("change", (e) => { st.roman = e.target.checked; run(); });
    $("#native", host).addEventListener("change", (e) => { st.native = e.target.checked; run(); });
    $("#gender", host).addEventListener("change", (e) => { st.gender = e.target.value; run(); });
    $("#copy", host)?.addEventListener("click", async (e) => { try { await navigator.clipboard.writeText($("#out", host).textContent); e.currentTarget.textContent = "✓"; toast(t("tr.copied"), 1500); } catch { /* clipboard blocked */ } });
    $("#say", host)?.addEventListener("click", async () => { try { audio?.pause(); audio = new Audio(await speak($("#out", host).textContent, st.dst, (st.gender || "Female").toLowerCase())); await audio.play(); } catch (e) { toast(msg(e)); } });
    $("#back", host)?.addEventListener("click", async () => {
      try { const r = await api("/api/translate", { method: "POST", body: { text: $("#out", host).textContent, src: st.dst, dst: st.detected && st.detected !== st.dst ? st.detected : "en" } }); st.back = r.translation; draw(); } catch (e) { toast(msg(e)); }
    });
    $("#mic", host)?.addEventListener("click", mic);
  }

  function swap() {
    const from = st.src === "auto" ? st.detected || "en" : st.src;
    const text = $("#out", host)?.textContent && st.out ? $("#out", host).textContent : st.text;
    st.src = st.dst; st.dst = from; st.text = st.out ? text : st.text; bump(st.dst); keep(); st.out = null; draw(); run();
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
