// Setu: a live interpreter for two people. Each sentence is translated, checked, and given a trust level
// before it is spoken. Low trust asks the speaker to confirm first.
import { html, mount, $, $$ } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { api, speak, ApiError } from "../api.js";
import { startRecording, micSupported } from "../recorder.js";
import { toast, sfx } from "../fx.js";
import { icon, mark } from "../icons.js";
import { pickLanguage, quality } from "../langpick.js";

const msg = (e) => (e instanceof ApiError ? (e.message === "network" ? t("err.network") : e.message) : e.message === "silent" ? t("err.mic") : e.message || t("err.generic"));
const LEVEL = { good: "ok", check: "wobble", confirm: "broke" };
const dirOf = (text) => ((text.match(/[؀-ۿݐ-ݿ]/g) || []).length > (text.match(/\p{L}/gu) || []).length / 2 ? "rtl" : "ltr");

export function setu(host, app) {
  const cfg = app.config;
  const langs = cfg.languages;
  const L = (c) => langs.find((l) => l.code === c);
  const pick = (c, fb) => (L(c) ? c : fb);
  const st = { a: pick(["te", "hi"].includes(getLang()) ? getLang() : "hi", "hi"), b: "en", domain: "general", careful: false, turns: [], gender: { a: "", b: "" }, rec: null, busy: false, ack: false };
  if (st.a === "en") st.b = "hi";
  let player = null;
  const dom = () => cfg.domains.find((d) => d.id === st.domain) || cfg.domains[0];
  const notes = (c) => L(c)?.notices || L("en").notices;

  function consent() {
    if (!dom().consent || st.ack) return "";
    const block = (c) => html`<div lang="${c}" dir="${L(c).script === "arabic" ? "rtl" : "ltr"}"><p class="hint">${L(c).native}</p><p style="margin-top:.3rem">${notes(c).ai_translation} ${notes(c).not_advice}</p><p class="hint">${notes(c).privacy}</p>
      ${L(c).voice ? html`<button class="btn line" type="button" data-say="${c}" style="margin-top:.4rem">${icon("speaker", 16)} ${t("s.play")}</button>` : ""}</div>`;
    return html`<div class="card stack"><h3 class="h3" style="font-size:1.2rem">${t("s.consent")}</h3><div class="grid2">${block(st.a)}${block(st.b)}</div><div class="row"><button class="btn" type="button" id="agree">${t("s.agree")}</button></div></div>`;
  }

  function turnRow(x) {
    const tr = x.turn.trust, issues = tr.issues.map((i) => t("s.i." + i)).join(" · ");
    return html`<article class="card stack" style="gap:.5rem" data-turn="${x.id}">
      <div class="row"><span class="hint">${L(x.from).native} → ${L(x.to).native}</span><span class="grow"></span>
        <span class="chip ${LEVEL[tr.level]}" title="${issues}">${mark(LEVEL[tr.level], 14)} ${t("s." + tr.level)} · ${tr.score}</span></div>
      <p class="muted" lang="${x.from}" dir="${dirOf(x.turn.source)}">${x.turn.source}</p>
      <p class="say" lang="${x.to}" dir="${dirOf(x.turn.translation)}">${x.turn.translation}</p>
      ${issues ? html`<p class="hint">${issues}</p>` : ""}
      ${x.turn.back_translation ? html`<details><summary class="hint" style="cursor:pointer">${t("s.back")}</summary><p class="muted" lang="${x.from}" dir="auto" style="margin-top:.3rem">${x.turn.back_translation}</p></details>` : ""}
      ${x.turn.needs_confirm && !x.confirmed ? html`<div class="err-box stack" style="gap:.5rem"><b style="font-weight:560">${t("s.q")}</b><p lang="${x.from}" dir="auto">${x.turn.back_translation || x.turn.source}</p>
        <div class="row"><button class="btn sm" type="button" data-go="${x.id}">${t("s.say")}</button><button class="btn line" type="button" data-redo="${x.id}">${t("s.redo")}</button></div></div>`
        : x.turn.can_speak ? html`<div class="row"><button class="btn line" type="button" data-play="${x.id}">${icon("play", 16)} ${t("s.play")}</button></div>` : html`<p class="hint">${t("s.novoice")}</p>`}
    </article>`;
  }

  function draw() {
    const d = dom();
    const side = (s) => { const l = L(s === "a" ? st.a : st.b); return html`<div class="stack" style="gap:.8rem">
      <p class="hint">${s === "a" ? t("s.a") : t("s.b")}</p><h3 class="h3" lang="${l.code}">${l.native}</h3>
      <div class="row">${micSupported() ? html`<button class="mic ${st.rec?.side === s ? "rec" : ""}" type="button" data-mic="${s}" aria-label="${t("s.tap")}" ${st.busy || (d.consent && !st.ack) ? "disabled" : ""}>${icon(st.rec?.side === s ? "stop" : "mic", 22)}</button>` : html`<span class="hint">${t("s.nomic")}</span>`}
        <span class="hint" data-note="${s}">${t("s.tap")}</span></div>
      <form class="row" data-type="${s}"><input class="input grow" placeholder="${t("s.type")}" lang="${l.code}" dir="auto" maxlength="${cfg.limits.turn_chars}"><button class="btn sm" type="submit" ${st.busy ? "disabled" : ""}>${t("s.send")}</button></form>
      <select class="input" data-gender="${s}" style="width:auto;font-size:.9rem" aria-label="gender"><option value="">—</option><option value="Female" ${st.gender[s] === "Female" ? "selected" : ""}>Female</option><option value="Male" ${st.gender[s] === "Male" ? "selected" : ""}>Male</option></select></div>`; };
    mount(host, html`<section class="sheet" style="padding-top:1.6rem"><div class="bench" style="width:min(100% - 2 * var(--gutter), 64rem)">
      <div class="bench-head"><div><h2>${t("s.title")}</h2><p class="muted" style="margin-top:.4rem;max-width:36rem">${t("s.sub")}</p></div></div>
      <div class="stack" style="gap:1.6rem">
        <div class="tr-top" style="margin:0"><button class="input" type="button" id="pick-a" lang="${st.a}" style="text-align:start;cursor:pointer">${L(st.a).native}</button>
          <button class="icon-btn" type="button" id="swap" aria-label="${t("s.swap")}" title="${t("s.swap")}">${icon("swap", 18)}</button>
          <button class="input" type="button" id="pick-b" lang="${st.b}" style="text-align:start;cursor:pointer">${L(st.b).native}</button>
          <span class="hint" style="font-weight:400">${[L(st.a), L(st.b)].map(quality).filter(Boolean)[0] || ""}</span></div>
        <div class="row"><label class="field" style="min-width:14rem">${t("s.domain")}<select class="input" id="dm">${cfg.domains.map((x) => html`<option value="${x.id}" ${x.id === st.domain ? "selected" : ""}>${x.label}</option>`)}</select></label>
          <label class="chip" style="cursor:pointer"><input type="checkbox" id="careful" ${st.careful ? "checked" : ""}> ${t("s.careful")}</label><span class="hint">${t("s.careful.d")}</span></div>
        ${consent()}
        <div class="grid2" style="border-top:1px solid var(--rule-strong);padding-top:1.4rem">${side("a")}${side("b")}</div>
        <div class="stack" id="log" aria-live="polite">${st.turns.length ? st.turns.map(turnRow) : html`<p class="muted">${t("s.empty")}</p>`}</div>
        ${st.turns.length ? html`<div class="row"><button class="btn line" type="button" id="clear">${t("s.clear")}</button></div>` : ""}
      </div></div></section>`);
    wire();
  }

  const setNote = (side, text) => { const n = $(`[data-note="${side}"]`, host); if (n) n.textContent = text; };

  async function play(x) {
    try {
      const gender = st.gender[x.side === "a" ? "b" : "a"] || "female";
      const url = await speak(x.turn.translation, x.to, gender.toLowerCase());
      player?.pause(); player = new Audio(url); await player.play();
    } catch (e) { toast(msg(e)); }
  }

  async function addTurn(side, request) {
    st.busy = true; setNote(side, t("s.thinking"));
    try {
      const from = side === "a" ? st.a : st.b, to = side === "a" ? st.b : st.a;
      let r;
      if (request.audio) {
        const f = new FormData();
        f.append("audio", request.audio, "clip.wav"); f.append("src", from); f.append("dst", to); f.append("careful", st.careful ? "1" : "0"); f.append("domain", st.domain); if (st.gender[side]) f.append("gender", st.gender[side]);
        r = await api("/api/live/utterance", { method: "POST", form: f });
        if (r.empty) { toast(t("err.mic")); st.busy = false; return draw(); }
        r = r.turn;
      } else {
        r = await api("/api/live/turn", { method: "POST", body: { text: request.text, src: from, dst: to, careful: st.careful, domain: st.domain, gender: st.gender[side] || undefined } });
      }
      const x = { id: crypto.randomUUID(), side, from, to, turn: r, confirmed: false };
      st.turns.push(x); st.busy = false; draw(); sfx("tap");
      if (!r.needs_confirm && r.can_speak) play(x);
    } catch (e) { st.busy = false; draw(); toast(msg(e)); }
  }

  async function mic(side, btn) {
    if (st.rec?.side === side) {
      const r = st.rec; st.rec = null; btn.classList.remove("rec"); setNote(side, t("s.thinking"));
      try { const { blob } = await r.handle.stop(); await addTurn(side, { audio: blob }); } catch (e) { draw(); toast(msg(e)); }
      return;
    }
    if (st.rec) return;
    try { const handle = await startRecording({ maxSeconds: 29, onAutoStop: () => mic(side, btn) }); st.rec = { side, handle }; btn.classList.add("rec"); setNote(side, t("s.hearing")); }
    catch { toast(t("err.mic")); }
  }

  function wire() {
    const choose = (key) => (e) => pickLanguage({ anchor: e.currentTarget, languages: langs, current: st[key], onPick: (c) => { st[key] = c; if (st.a === st.b) { const o = key === "a" ? "b" : "a"; st[o] = c === "en" ? "hi" : "en"; } draw(); } });
    $("#pick-a", host).addEventListener("click", choose("a"));
    $("#pick-b", host).addEventListener("click", choose("b"));
    $("#swap", host).addEventListener("click", () => { [st.a, st.b] = [st.b, st.a]; draw(); });
    $("#dm", host).addEventListener("change", (e) => { st.domain = e.target.value; st.careful = dom().careful; st.ack = false; draw(); });
    $("#careful", host).addEventListener("change", (e) => { st.careful = e.target.checked; });
    $("#agree", host)?.addEventListener("click", () => { st.ack = true; draw(); });
    $$("[data-say]", host).forEach((b) => b.addEventListener("click", async () => { try { const c = b.dataset.say; const a = new Audio(await speak(`${notes(c).ai_translation} ${notes(c).not_advice}`, c, "female")); await a.play(); } catch (e) { toast(msg(e)); } }));
    $$("[data-mic]", host).forEach((b) => b.addEventListener("click", () => mic(b.dataset.mic, b)));
    $$("[data-type]", host).forEach((f) => f.addEventListener("submit", (e) => { e.preventDefault(); const v = $("input", f).value.trim(); if (v && !st.busy) addTurn(f.dataset.type, { text: v }); }));
    $$("[data-gender]", host).forEach((s) => s.addEventListener("change", () => { st.gender[s.dataset.gender] = s.value; }));
    $$("[data-play]", host).forEach((b) => b.addEventListener("click", () => play(st.turns.find((x) => x.id === b.dataset.play))));
    $$("[data-go]", host).forEach((b) => b.addEventListener("click", () => { const x = st.turns.find((y) => y.id === b.dataset.go); x.confirmed = true; draw(); if (x.turn.can_speak) play(x); }));
    $$("[data-redo]", host).forEach((b) => b.addEventListener("click", () => { st.turns = st.turns.filter((y) => y.id !== b.dataset.redo); draw(); }));
    $("#clear", host)?.addEventListener("click", () => { st.turns = []; draw(); });
  }

  draw();
  return () => { st.rec?.handle.cancel(); player?.pause(); };
}
