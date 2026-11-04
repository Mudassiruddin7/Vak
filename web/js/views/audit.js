import { html, mount, el, $, $$, esc, raw, sleep, formatInr } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { api, stream, speak, ApiError } from "../api.js";
import * as S from "../state.js";
import * as auth from "../auth.js";
import { startRecording, micSupported } from "../recorder.js";
import { highlightChanges } from "../diff.js";
import { VoicePlayer } from "../player.js";
import { MITTHU_SVG } from "../mascot-svg.js";
import { toast, confetti, sfx } from "../fx.js";

const SAMPLES = {
  te: "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?",
  hi: "दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं?",
  en: "Which is bigger, 9.11 or 9.9?",
};
const LEVEL_COLOR = { easy: "ok", medium: "wobble", hard: "broke" };
const message = (e) => (e instanceof ApiError ? (e.message === "network" ? t("err.network") : e.message) : e.message || t("err.generic"));

export function audit(host, app) {
  const cfg = app.config;
  const st = { text: "", effort: "auto", targetKey: "sarvam:sarvam-105b-conversations", custom: { url: "", model: "", key: "" }, system: "", plan: null, diff: null, est: null,
    explainLang: ["te", "hi", "en"].includes(getLang()) ? getLang() : "te", abort: null, recorder: null, player: null };
  const spec = () => {
    const [kind, model] = st.targetKey.split(":");
    return kind === "custom" ? { kind, model: "custom", base_url: st.custom.url, remote_model: st.custom.model, api_key: st.custom.key || undefined, system_prompt: st.system || undefined }
      : { kind, model, system_prompt: st.system || undefined };
  };

  function cleanup() { st.abort?.abort(); st.recorder?.cancel(); st.player?.destroy(); }

  // ---- ask -------------------------------------------------------------
  function ask(error = "") {
    const lang = getLang();
    const models = cfg.models.map((m) => html`<option value="sarvam:${m.id}" ${st.targetKey === `sarvam:${m.id}` ? "selected" : ""}>${m.label}</option>`);
    const practice = cfg.practice.map((m) => html`<option value="demo:${m.id}" ${st.targetKey === `demo:${m.id}` ? "selected" : ""}>${m.label}</option>`);
    mount(host, html`
      <div class="bench-head"><h2>${t("a.title")}</h2><button class="btn sm line" type="button" id="demo">${t("a.demo")}</button></div>
      <div class="stack">
        <label class="field"><span class="sr">${t("a.ph")}</span>
          <textarea class="input" id="q" lang="${/[ఀ-౿]/.test(st.text) ? "te" : /[ऀ-ॿ]/.test(st.text) ? "hi" : lang}" placeholder="${t("a.ph")}" maxlength="${cfg.limits.prompt_chars * 2 || 1200}">${st.text}</textarea></label>
        <div class="row">
          ${micSupported() ? html`<button class="mic" type="button" id="mic" aria-label="${t("a.mic")}"><span class="bars"><i></i><i></i><i></i><i></i><i></i></span></button><span class="hint" id="mic-note">${t("a.mic")}</span>` : ""}
          <span class="grow"></span><span class="hint">${t("a.examples")}:</span>
          <span class="chips">${["te", "hi", "en"].map((c) => html`<button class="chip" type="button" data-sample="${c}" lang="${c}">${SAMPLES[c].slice(0, 22)}…</button>`)}</span>
        </div>
        <div class="grid2">
          <label class="field">${t("a.target")}
            <select class="input" id="target"><optgroup label="Sarvam">${models}</optgroup><optgroup label="${t("a.practice")}">${practice}</optgroup>${cfg.custom_allowed ? html`<option value="custom:x" ${st.targetKey.startsWith("custom") ? "selected" : ""}>${t("a.custom")}</option>` : ""}</select></label>
          <div class="field">${t("a.effort")}
            <div class="seg" id="effort" role="group">${["auto", "light", "balanced", "thorough"].map((e) => html`<button type="button" data-e="${e}" class="${st.effort === e ? "on" : ""}">${e === "auto" ? t("a.auto") : t("effort." + e)}</button>`)}</div>
            <span class="hint">${st.effort === "auto" ? "✦ " + t("a.autohint") : t("effort." + st.effort + ".d")}</span></div>
        </div>
        ${st.targetKey.startsWith("custom") ? html`<div class="grid2">
          <label class="field">URL<input class="input" id="c-url" placeholder="http://localhost:11434/v1" value="${st.custom.url}"></label>
          <label class="field">Model<input class="input" id="c-model" value="${st.custom.model}"></label>
          <label class="field">API key<input class="input" id="c-key" type="password" autocomplete="off" value="${st.custom.key}"></label></div>` : ""}
        <details><summary class="hint" style="cursor:pointer">${t("a.sys")}</summary><textarea class="input" id="sys" style="font:inherit;min-height:4rem;margin-top:.5rem">${st.system}</textarea></details>
        ${error ? html`<div class="err-box" role="alert">${error}</div>` : ""}
        <div class="row"><button class="btn" type="button" id="go">${t("a.plan")} →</button></div>
      </div>`);
    const q = $("#q", host);
    q.addEventListener("input", () => { st.text = q.value; });
    $$("[data-sample]", host).forEach((b) => b.addEventListener("click", () => { st.text = SAMPLES[b.dataset.sample]; ask(); }));
    $("#target", host).addEventListener("change", (e) => { st.targetKey = e.target.value; keep(); ask(); });
    $$("#effort [data-e]", host).forEach((b) => b.addEventListener("click", () => { st.effort = b.dataset.e; keep(); ask(); }));
    $("#demo", host).addEventListener("click", () => demo("steady"));
    $("#go", host).addEventListener("click", plan);
    $("#mic", host)?.addEventListener("click", toggleMic);
    q.addEventListener("keydown", (e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) plan(); });
  }
  function keep() {
    st.system = $("#sys", host)?.value ?? st.system; st.text = $("#q", host)?.value ?? st.text;
    st.custom = { url: $("#c-url", host)?.value ?? st.custom.url, model: $("#c-model", host)?.value ?? st.custom.model, key: $("#c-key", host)?.value ?? st.custom.key };
  }

  async function toggleMic() {
    const mic = $("#mic", host), note = $("#mic-note", host);
    if (st.recorder) {
      mic.classList.remove("rec"); note.textContent = t("state.waiting");
      const r = st.recorder; st.recorder = null;
      try {
        const { blob } = await r.stop();
        note.textContent = t("bubble.transcribing");
        const form = new FormData(); form.append("audio", blob, "clip.wav"); form.append("lang", "auto");
        const out = await api("/api/transcribe", { method: "POST", form });
        st.text = out.text; ask();
      } catch (e) { ask(e.message === "silent" ? t("err.mic") : message(e)); }
      return;
    }
    try {
      keep();
      st.recorder = await startRecording({ onFrame: (bins) => { const l = Math.min(1, bins.slice(2, 20).reduce((a, b) => a + b, 0) / 18 / 160); $$(".bars i", mic).forEach((i, n) => { i.style.height = `${30 + l * 70 * (n % 2 ? 1 : .7)}%`; }); }, onTick: (s) => { note.textContent = `${s.toFixed(0)}s`; }, onAutoStop: toggleMic });
      mic.classList.add("rec"); note.textContent = t("bubble.listening");
    } catch { ask(t("err.mic")); }
  }

  // ---- plan ------------------------------------------------------------
  async function plan(effortOverride) {
    keep();
    if (!st.text.trim()) return ask(t("a.ph"));
    mount(host, html`<div class="mascot-row"><div class="mitthu-wrap">${raw(MITTHU_SVG)}</div><div class="bubble">${t("bubble.transcribing")}</div></div>`);
    try {
      const body = { text: st.text, target: spec() };
      const eff = typeof effortOverride === "string" ? effortOverride : st.effort;
      if (eff !== "auto") body.effort = eff;
      const r = await api("/api/plan", { method: "POST", body });
      st.plan = r.plan; st.diff = r.difficulty; st.est = r.estimate; st.effortShown = r.plan.effort;
      planView();
    } catch (e) { ask(message(e)); }
  }
  function planView() {
    const p = st.plan, d = st.diff, e = st.est;
    mount(host, html`
      <div class="bench-head"><h2>${t("a.plan")}</h2><button class="btn sm line" type="button" id="back">← ${t("a.edit")}</button></div>
      <div class="stack">
        <div class="card"><p class="eyebrow">${t("plan.prompt")}</p><p class="say" lang="${p.prompt_language}" style="margin-top:.4rem">${p.prompt}</p></div>
        <div class="grid2">
          <div class="card stack"><div class="row"><span class="eyebrow">${t("a.diff")}</span><span class="chip ${LEVEL_COLOR[d.level]}">${t("diff." + d.level)}</span></div>
            <div class="chips">${d.reasons.map((r) => html`<span class="chip">${t("diff." + r)}</span>`)}</div></div>
          <div class="card stack"><span class="eyebrow">${t("a.effort")}</span>
            <div class="seg" id="effort2" role="group">${["light", "balanced", "thorough"].map((x) => html`<button type="button" data-e="${x}" class="${p.effort === x ? "on" : ""}">${t("effort." + x)}</button>`)}</div>
            <span class="hint">${t("effort." + p.effort + ".d")}</span></div>
        </div>
        <div class="card row"><span class="eyebrow">${t("a.est")}</span><b>${formatInr(e.inr_low)} – ${formatInr(e.inr_high)}</b><span class="muted">· ${e.calls_low}–${e.calls_high} ${t("a.calls")}</span>
          <span class="grow"></span><span class="chip">${cfg.models.find((m) => m.id === p.target.model)?.label || p.target.model}</span></div>
        ${p.notes.length ? html`<div class="hint">${p.notes.join(" ")}</div>` : ""}
        <div class="row"><button class="btn go" type="button" id="run">${t("a.run")} →</button>
          <span class="chips" role="group" aria-label="${t("explain.title")}">${["te", "hi", "en"].map((c) => html`<button class="chip ${st.explainLang === c ? "on" : ""}" type="button" data-xl="${c}">${c === "te" ? "తెలుగు" : c === "hi" ? "हिन्दी" : "English"}</button>`)}</span></div>
      </div>`);
    $("#back", host).addEventListener("click", () => ask());
    $$("#effort2 [data-e]", host).forEach((b) => b.addEventListener("click", () => plan(b.dataset.e)));
    $$("[data-xl]", host).forEach((b) => b.addEventListener("click", () => { st.explainLang = b.dataset.xl; planView(); }));
    $("#run", host).addEventListener("click", run);
  }

  // ---- run -------------------------------------------------------------
  async function run() {
    st.abort = new AbortController();
    const cards = new Map();
    const state = { done: 0, total: 1, phase: "variants", report: null, expl: null, sig: null };
    mount(host, html`
      <div class="bench-head"><h2>${t("a.run")}</h2><button class="btn sm line" type="button" id="stop">${t("state.cancel") === "state.cancel" ? "Stop" : t("state.cancel")}</button></div>
      <div class="stack">
        <div class="mascot-row"><div class="ring-prog" id="ring" data-label="0%"></div><div class="stack" style="gap:.4rem"><div class="bubble" id="say">${t("phase.variants")}</div><div class="hint" id="base"></div></div></div>
        <div class="run-grid" id="grid"></div>
      </div>`);
    const say = (k) => { $("#say", host).textContent = t("phase." + k); };
    const ring = $("#ring", host), grid = $("#grid", host);
    $("#stop", host).addEventListener("click", () => { st.abort.abort(); ask(); });
    const setProgress = () => { const p = Math.round((state.done / Math.max(1, state.total)) * 100); ring.style.setProperty("--p", p); ring.dataset.label = `${p}%`; };
    const card = (v) => {
      const c = el("div", { class: `run-card ${v.standby ? "standby" : ""}`, dataset: { id: v.id } });
      mount(c, html`<div class="row"><b>${t("cat." + v.category) === "cat." + v.category ? v.category : t("cat." + v.category)}</b><span class="grow"></span><span class="chip" data-s>${v.standby ? "·" : t("state.waiting")}</span></div><div class="muted" style="font-size:.85rem">${v.label}</div>`);
      cards.set(v.id, c); grid.append(c);
    };
    try {
      await stream("/api/audits/stream", { plan: st.plan, language: st.explainLang }, (name, d) => {
        if (name === "phase") { state.phase = d.name; if (d.name !== "explain" || true) say(d.name); }
        else if (name === "plan") { d.variants.forEach(card); }
        else if (name === "baseline") { $("#base", host).textContent = d.output.slice(0, 160); }
        else if (name === "progress") { state.done = d.done; state.total = d.total; setProgress(); }
        else if (name === "answer") { const c = cards.get(d.id) || (card({ id: d.id, category: "resample", label: t("cat.resample") }), cards.get(d.id)); c.classList.remove("standby"); $("[data-s]", c).textContent = d.error ? t("state.error") : t("state.answered"); }
        else if (name === "test") { const c = cards.get(d.id); if (c) { const s = $("[data-s]", c); s.textContent = t("state." + d.status); s.className = `chip ${d.status}`; c.classList.remove("standby"); } }
        else if (name === "report") { state.report = d.report; state.sig = d.sig; }
        else if (name === "explanation") { state.expl = d; }
        else if (name === "error") { throw new ApiError(d.message, d.code || 0); }
      }, st.abort.signal);
      if (!state.report) throw new ApiError(t("err.generic"));
      result(state.report, state.expl ? { [state.explainLangUsed || st.explainLang]: state.expl } : {}, state.sig);
    } catch (e) { if (e.name !== "AbortError") ask(message(e)); }
  }

  async function demo(name) {
    try { const d = await api(`/api/demo/${name}`); result(d.report, d.explanations, null, true); }
    catch (e) { ask(message(e)); }
  }

  // ---- result ----------------------------------------------------------
  function result(report, explanations, sig, isDemo = false) {
    st.player?.destroy();
    const lang = explanations[st.explainLang] ? st.explainLang : Object.keys(explanations)[0] || st.explainLang;
    st.explainLang = lang;
    const grade = report.grade, pct = Math.round(report.consistency_score * 100);
    const color = grade === "steady" ? "var(--fathom)" : grade === "mixed" ? "var(--glow)" : "var(--pulse)";
    const circ = 2 * Math.PI * 52;
    const ex = explanations[lang];
    const flags = report.flags.length ? report.flags.map((f) => html`<div class="card"><div class="row"><b>${f.title}</b><span class="chip ${f.severity === "high" ? "broke" : f.severity === "medium" ? "wobble" : ""}">${f.severity}</span></div><p class="muted">${f.detail}</p></div>`) : html`<p class="muted">${t("flags.none")}</p>`;
    mount(host, html`
      <div class="bench-head"><h2>${t("a.title")}</h2><div class="row"><button class="btn sm line" id="again" type="button">${t("a.again")}</button></div></div>
      <div class="stack" style="gap:1.8rem">
        <div class="score">
          <div class="gauge"><svg viewBox="0 0 120 120"><circle cx="60" cy="60" r="52" fill="none" stroke="var(--lumen-dark)" stroke-width="10"/><circle id="arc" cx="60" cy="60" r="52" fill="none" stroke="${color}" stroke-width="10" stroke-linecap="round" stroke-dasharray="${circ}" stroke-dashoffset="${circ}"/></svg><div class="num"><span id="pct">0</span><small>/ 100</small></div></div>
          <div class="stack" style="gap:.7rem"><h3 class="verdict">${t("grade." + grade)}</h3>
            <div class="chips"><span class="chip">${report.total_perturbations} ${t("stat.tests")}</span><span class="chip ${report.unexpected_changes ? "wobble" : "ok"}">${report.unexpected_changes} ${t("stat.unexpected")}</span>${report.seconds ? html`<span class="chip">${report.seconds}s</span>` : ""}${report.cost_inr ? html`<span class="chip">${formatInr(report.cost_inr)}</span>` : ""}${report.calls ? html`<span class="chip">${report.calls.target + report.calls.helpers} ${t("a.calls")}</span>` : ""}</div>
            <p class="muted" lang="${report.prompt_language}">“${report.prompt}”</p></div>
        </div>
        <section class="card stack"><div class="row"><h3 class="h3" style="font-size:1.5rem">${t("explain.title")}</h3><span class="grow"></span>
          ${Object.keys(explanations).length > 1 || !isDemo ? html`<span class="chips" id="xl">${(app.config.languages.filter((l) => l.explains).map((l) => l.code)).filter((c) => !isDemo || explanations[c]).map((c) => html`<button class="chip ${c === lang ? "on" : ""}" type="button" data-xl="${c}">${app.config.languages.find((l) => l.code === c)?.native || c}</button>`)}</span>` : ""}</div>
          ${ex ? html`<p class="say" id="say" lang="${lang}">${ex.sentences.map((s, i) => html`<s data-i="${i}">${s} </s>`)}</p>
            ${ex.unreviewed ? html`<p class="hint">${t("a.unrev")}</p>` : ""}${ex.source === "template" ? html`<p class="hint">${t("explain.template")}</p>` : ""}
            <div class="row"><button class="btn sm" id="listen" type="button">▶ ${t("a.listen")}</button><button class="btn sm line" id="copy" type="button">${t("a.copy")}</button>${auth.user() && !isDemo ? html`<button class="btn sm line" id="save" type="button">${t("a.save")}</button>` : ""}<span class="hint" id="voice-note"></span></div>`
            : html`<p class="muted">…</p>`}
        </section>
        <section class="stack"><h3 class="h3" style="font-size:1.5rem">${t("flags.title")}</h3><div class="grid2">${flags}</div></section>
        <section class="stack"><div class="stack">${report.groups.map((g) => html`<div class="bar-row"><span>${t("grp." + g.key)}</span><div class="meter"><i style="width:${Math.round(g.score * 100)}%"></i></div><b>${Math.round(g.score * 100)}</b></div>`)}</div></section>
        ${report.units.length ? html`<section class="stack"><h3 class="h3" style="font-size:1.5rem">${t("words.title")}</h3><p class="say heat" lang="${report.prompt_language}">${report.units.map((u) => html`<span style="--w:${u.importance}">${u.text}</span>`)}</p><p class="hint">${t("words.hint")}</p></section>` : ""}
        <section class="stack"><h3 class="h3" style="font-size:1.5rem">${t("tried.title")}</h3><div class="tests">${report.tests.map((x) => html`<details class="test"><summary><span>${x.label}</span><span class="chip ${x.status}">${t("state." + x.status)}</span></summary>
          <div class="body"><div><span class="eyebrow">${t("detail.input")}</span><q class="diff">${highlightChanges(report.prompt, x.input)}</q></div>${x.output ? html`<div><span class="eyebrow">${t("detail.output")}</span><q>${x.output}</q></div>` : ""}${x.reason ? html`<div class="hint">${t("detail.why")}: ${x.reason}</div>` : ""}</div></details>`)}</div></section>
        <section class="grid2"><div class="card stack"><h4 class="h3" style="font-size:1.25rem">${t("untested.title")}</h4><ul class="muted" style="margin:0;padding-left:1.1rem">${report.not_tested.map((n) => html`<li>${n}</li>`)}</ul></div>
          <div class="card stack"><h4 class="h3" style="font-size:1.25rem">${t("plan.next") === "plan.next" ? "Next steps" : t("plan.next")}</h4><ul class="muted" style="margin:0;padding-left:1.1rem">${report.next_steps.map((n) => html`<li>${n}</li>`)}</ul></div></section>
        ${report.notes.length ? html`<p class="hint">${report.notes.join(" ")}</p>` : ""}
      </div>`);
    // numbers settle in instead of appearing
    const arc = $("#arc", host), pctEl = $("#pct", host);
    requestAnimationFrame(() => { arc.style.transition = "stroke-dashoffset 1.1s cubic-bezier(.2,.8,.2,1)"; arc.style.strokeDashoffset = circ * (1 - report.consistency_score); });
    let n = 0; const tick = () => { n = Math.min(pct, n + Math.max(1, Math.round(pct / 30))); pctEl.textContent = n; if (n < pct) requestAnimationFrame(tick); }; tick();
    if (grade === "steady" && !isDemo) { confetti(); sfx("done"); } else sfx("tap");
    if (!isDemo) S.earn(10);
    $("#again", host).addEventListener("click", () => ask());
    $$("#xl [data-xl]", host).forEach((b) => b.addEventListener("click", async () => {
      const c = b.dataset.xl;
      if (explanations[c]) { st.explainLang = c; return result(report, explanations, sig, isDemo); }
      if (!sig) return;
      b.textContent = "…";
      try { explanations[c] = await api("/api/explain", { method: "POST", body: { report, sig, language: c } }); st.explainLang = c; result(report, explanations, sig, isDemo); }
      catch (e) { toast(message(e)); b.textContent = app.config.languages.find((l) => l.code === c)?.native || c; }
    }));
    if (ex) {
      const sentences = $$("#say s", host);
      $("#copy", host).addEventListener("click", async () => { try { await navigator.clipboard.writeText(`${report.prompt}\n${pct}/100 ${t("grade." + grade)}\n\n${ex.text}`); toast(t("a.copied")); } catch { /* clipboard blocked */ } });
      $("#save", host)?.addEventListener("click", async () => { try { await auth.saveAudit({ prompt: report.prompt, score: report.consistency_score, grade, report, explanation: ex, lang }); toast(t("a.saved")); } catch (e) { toast(e.message); } });
      const listen = $("#listen", host), note = $("#voice-note", host);
      const total = ex.sentences.reduce((a, s) => a + s.length, 0);
      st.player = new VoicePlayer({
        onTime: (cur, dur) => { if (!dur) return; let at = (cur / dur) * total, acc = 0, idx = 0; ex.sentences.forEach((s, i) => { if (at >= acc) idx = i; acc += s.length; }); sentences.forEach((s, i) => s.classList.toggle("now", i === idx)); },
        onState: (s) => { listen.textContent = s === "playing" ? `❚❚ ${t("a.pause")}` : `▶ ${t("a.listen")}`; if (s === "ended") sentences.forEach((x) => x.classList.remove("now")); },
      });
      listen.addEventListener("click", async () => {
        if (st.player.playing) return st.player.pause();
        try { note.textContent = t("explain.preparing"); await st.player.load(await speak(ex.text.slice(0, 1700), lang, "female")); note.textContent = ""; await st.player.play(); }
        catch (e) { note.textContent = e.message === "network" ? t("explain.voiceFail") : message(e); }
      });
    }
  }

  ask();
  return cleanup;
}
