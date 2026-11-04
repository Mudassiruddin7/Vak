import { html, mount, el, $, $$, formatInr } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { api, stream, speak, ApiError } from "../api.js";
import * as S from "../state.js";
import * as auth from "../auth.js";
import { startRecording, micSupported } from "../recorder.js";
import { highlightChanges } from "../diff.js";
import { VoicePlayer } from "../player.js";
import { icon, mark } from "../icons.js";
import { pickLanguage, quality } from "../langpick.js";
import { toast, sfx } from "../fx.js";

const SAMPLES = {
  te: "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?",
  hi: "दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं?",
  en: "Which is bigger, 9.11 or 9.9?",
};
const message = (e) => (e instanceof ApiError ? (e.message === "network" ? t("err.network") : e.message) : e.message || t("err.generic"));
const dirOf = (text) => ((text.match(/[؀-ۿݐ-ݿ]/g) || []).length > (text.match(/\p{L}/gu) || []).length / 2 ? "rtl" : "ltr");

export function audit(host, app) {
  const cfg = app.config;
  const L = (c) => cfg.languages.find((l) => l.code === c);
  const explainLangs = () => cfg.languages.filter((l) => l.explains).map((l) => l.code);
  const st = { text: "", effort: "auto", targetKey: "sarvam:sarvam-105b-conversations", custom: { url: "", model: "", key: "" }, system: "", plan: null, diff: null, est: null,
    explainLang: explainLangs().includes(getLang()) ? getLang() : "te", abort: null, recorder: null, player: null };
  const spec = () => {
    const [kind, model] = st.targetKey.split(":");
    return kind === "custom" ? { kind, model: "custom", base_url: st.custom.url, remote_model: st.custom.model, api_key: st.custom.key || undefined, system_prompt: st.system || undefined }
      : { kind, model, system_prompt: st.system || undefined };
  };
  const cleanup = () => { st.abort?.abort(); st.recorder?.cancel(); st.player?.destroy(); };
  const langButton = (id) => html`<button class="btn line" type="button" id="${id}" lang="${st.explainLang}">${L(st.explainLang)?.native || st.explainLang}</button>`;
  const pickExplain = (anchor, done) => pickLanguage({ anchor, languages: cfg.languages, only: explainLangs(), current: st.explainLang, onPick: (c) => { st.explainLang = c; done(c); } });

  // ---- ask ---------------------------------------------------------------
  function ask(error = "") {
    const lang = getLang();
    mount(host, html`
      <div class="bench-head"><h2>${t("a.title")}</h2><button class="btn line" type="button" id="demo">${t("a.demo")}</button></div>
      <div class="stack">
        <label class="field"><span class="sr">${t("a.ph")}</span>
          <textarea class="input" id="q" dir="auto" lang="${/[ఀ-౿]/.test(st.text) ? "te" : /[ऀ-ॿ]/.test(st.text) ? "hi" : lang}" placeholder="${t("a.ph")}" maxlength="${cfg.limits.prompt_chars * 2 || 1200}">${st.text}</textarea></label>
        <div class="row">
          ${micSupported() ? html`<button class="mic" type="button" id="mic" aria-label="${t("a.mic")}">${icon("mic", 22)}</button><span class="hint" id="mic-note">${t("a.mic")}</span>` : ""}
          <span class="grow"></span><span class="hint">${t("a.examples")}:</span>
          ${["te", "hi", "en"].map((c) => html`<button class="link" type="button" data-sample="${c}" lang="${c}">${SAMPLES[c].slice(0, 20)}…</button>`)}
        </div>
        <div class="grid2">
          <label class="field">${t("a.target")}
            <select class="input" id="target"><optgroup label="Sarvam">${cfg.models.map((m) => html`<option value="sarvam:${m.id}" ${st.targetKey === `sarvam:${m.id}` ? "selected" : ""}>${m.label}</option>`)}</optgroup>
              <optgroup label="${t("a.practice")}">${cfg.practice.map((m) => html`<option value="demo:${m.id}" ${st.targetKey === `demo:${m.id}` ? "selected" : ""}>${m.label}</option>`)}</optgroup>${cfg.custom_allowed ? html`<option value="custom:x" ${st.targetKey.startsWith("custom") ? "selected" : ""}>${t("a.custom")}</option>` : ""}</select></label>
          <div class="field">${t("a.effort")}
            <div class="seg" id="effort" role="group">${["auto", "light", "balanced", "thorough"].map((e) => html`<button type="button" data-e="${e}" class="${st.effort === e ? "on" : ""}">${e === "auto" ? t("a.auto") : t("effort." + e)}</button>`)}</div>
            <span class="hint">${st.effort === "auto" ? t("a.autohint") : t("effort." + st.effort + ".d")}</span></div>
        </div>
        ${st.targetKey.startsWith("custom") ? html`<div class="grid2">
          <label class="field">URL<input class="input" id="c-url" placeholder="http://localhost:11434/v1" value="${st.custom.url}"></label>
          <label class="field">Model<input class="input" id="c-model" value="${st.custom.model}"></label>
          <label class="field">API key<input class="input" id="c-key" type="password" autocomplete="off" value="${st.custom.key}"></label></div>` : ""}
        <details><summary class="hint" style="cursor:pointer">${t("a.sys")}</summary><textarea class="input" id="sys" style="font-size:1rem;min-height:4rem;margin-top:.5rem">${st.system}</textarea></details>
        ${error ? html`<div class="err-box" role="alert">${error}</div>` : ""}
        <div class="row"><button class="btn" type="button" id="go">${t("a.plan")}</button></div>
      </div>`);
    const q = $("#q", host);
    q.addEventListener("input", () => { st.text = q.value; });
    $$("[data-sample]", host).forEach((b) => b.addEventListener("click", () => { st.text = SAMPLES[b.dataset.sample]; ask(); }));
    $("#target", host).addEventListener("change", (e) => { keep(); st.targetKey = e.target.value; ask(); });
    $$("#effort [data-e]", host).forEach((b) => b.addEventListener("click", () => { keep(); st.effort = b.dataset.e; ask(); }));
    $("#demo", host).addEventListener("click", () => demo("steady"));
    $("#go", host).addEventListener("click", () => plan());
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
      mic.classList.remove("rec"); note.textContent = t("bubble.transcribing");
      const r = st.recorder; st.recorder = null;
      try {
        const { blob } = await r.stop();
        const form = new FormData(); form.append("audio", blob, "clip.wav"); form.append("lang", "auto");
        const out = await api("/api/transcribe", { method: "POST", form });
        st.text = out.text; ask();
      } catch (e) { ask(e.message === "silent" ? t("err.mic") : message(e)); }
      return;
    }
    try {
      keep();
      st.recorder = await startRecording({ onTick: (s) => { note.textContent = `${s.toFixed(0)}s`; }, onAutoStop: toggleMic });
      mic.classList.add("rec"); note.textContent = t("bubble.listening");
    } catch { ask(t("err.mic")); }
  }

  // ---- plan --------------------------------------------------------------
  async function plan(effortOverride) {
    keep();
    if (!st.text.trim()) return ask(t("a.ph"));
    mount(host, html`<p class="muted" role="status">${t("bubble.transcribing")}</p>`);
    try {
      const body = { text: st.text, target: spec() };
      const eff = typeof effortOverride === "string" ? effortOverride : st.effort;
      if (eff !== "auto") body.effort = eff;
      const r = await api("/api/plan", { method: "POST", body });
      st.plan = r.plan; st.diff = r.difficulty; st.est = r.estimate;
      planView();
    } catch (e) { ask(message(e)); }
  }
  function planView() {
    const p = st.plan, d = st.diff, e = st.est;
    mount(host, html`
      <div class="bench-head"><h2>${t("a.plan")}</h2><button class="btn line" type="button" id="back">${icon("back", 16)} ${t("a.edit")}</button></div>
      <div class="stack">
        <div class="card"><p class="hint">${t("plan.prompt")}</p><p class="say" lang="${p.prompt_language}" dir="auto" style="margin-top:.3rem">${p.prompt}</p></div>
        <div class="grid2">
          <div class="card stack" style="gap:.6rem"><p class="hint">${t("a.diff")}</p><p><b style="font-weight:560">${t("diff." + d.level)}</b>${d.reasons.length ? html` <span class="muted">· ${d.reasons.map((r) => t("diff." + r)).join(", ")}</span>` : ""}</p></div>
          <div class="card stack" style="gap:.6rem"><p class="hint">${t("a.effort")}</p>
            <div class="seg" id="effort2" role="group">${["light", "balanced", "thorough"].map((x) => html`<button type="button" data-e="${x}" class="${p.effort === x ? "on" : ""}">${t("effort." + x)}</button>`)}</div>
            <p class="hint">${t("effort." + p.effort + ".d")}</p></div>
        </div>
        <div class="card row"><span class="hint">${t("a.est")}</span><b style="font-weight:560">${formatInr(e.inr_low)} – ${formatInr(e.inr_high)}</b><span class="muted">· ${e.calls_low}–${e.calls_high} ${t("a.calls")}</span>
          <span class="grow"></span><span class="muted">${cfg.models.find((m) => m.id === p.target.model)?.label || p.target.model}</span></div>
        ${p.notes.length ? html`<p class="hint">${p.notes.join(" ")}</p>` : ""}
        <div class="row"><button class="btn" type="button" id="run">${t("a.run")}</button><span class="grow"></span><span class="hint">${t("explain.title")}:</span>${langButton("xl-pick")}</div>
      </div>`);
    $("#back", host).addEventListener("click", () => ask());
    $$("#effort2 [data-e]", host).forEach((b) => b.addEventListener("click", () => plan(b.dataset.e)));
    $("#xl-pick", host).addEventListener("click", (ev) => pickExplain(ev.currentTarget, () => planView()));
    $("#run", host).addEventListener("click", run);
  }

  // ---- run ---------------------------------------------------------------
  async function run() {
    st.abort = new AbortController();
    const rows = new Map();
    const state = { done: 0, total: 1, report: null, expl: null, sig: null };
    mount(host, html`
      <div class="bench-head"><h2>${t("a.run")}</h2><button class="btn line" type="button" id="stop">${t("state.cancel") === "state.cancel" ? "Stop" : t("state.cancel")}</button></div>
      <div class="stack"><div class="progress"><p id="say" role="status">${t("phase.variants")}</p><div class="meter"><i id="bar" style="width:3%"></i></div><p class="hint" id="base"></p></div>
        <div class="run-grid" id="grid"></div></div>`);
    const grid = $("#grid", host);
    $("#stop", host).addEventListener("click", () => { st.abort.abort(); ask(); });
    const progress = () => { $("#bar", host).style.width = `${Math.max(3, Math.round((state.done / Math.max(1, state.total)) * 100))}%`; };
    const row = (v) => {
      const r = el("div", { class: `run-card ${v.standby ? "standby" : ""}`, dataset: { id: v.id } });
      mount(r, html`<span>${t("cat." + v.category) === "cat." + v.category ? v.category : t("cat." + v.category)} <span class="hint">· ${v.label}</span></span><span data-s>${mark("error")}</span>`);
      rows.set(v.id, r); grid.append(r);
    };
    try {
      await stream("/api/audits/stream", { plan: st.plan, language: st.explainLang }, (name, d) => {
        if (name === "phase") $("#say", host).textContent = t("phase." + d.name);
        else if (name === "plan") d.variants.forEach(row);
        else if (name === "baseline") $("#base", host).textContent = d.output.slice(0, 160);
        else if (name === "progress") { state.done = d.done; state.total = d.total; progress(); }
        else if (name === "answer") { (rows.get(d.id) || (row({ id: d.id, category: "resample", label: t("cat.resample") }), rows.get(d.id))).classList.remove("standby"); }
        else if (name === "test") { const r = rows.get(d.id); if (r) { $("[data-s]", r).replaceChildren(...mount(el("span"), mark(d.status)).childNodes); r.classList.remove("standby"); } }
        else if (name === "report") { state.report = d.report; state.sig = d.sig; }
        else if (name === "explanation") state.expl = d;
        else if (name === "error") throw new ApiError(d.message, d.code || 0);
      }, st.abort.signal);
      if (!state.report) throw new ApiError(t("err.generic"));
      result(state.report, state.expl ? { [st.explainLang]: state.expl } : {}, state.sig);
    } catch (e) { if (e.name !== "AbortError") ask(message(e)); }
  }

  async function demo(name) {
    try { const d = await api(`/api/demo/${name}`); result(d.report, d.explanations, null, true); }
    catch (e) { ask(message(e)); }
  }

  // ---- result ------------------------------------------------------------
  function result(report, explanations, sig, isDemo = false) {
    st.player?.destroy();
    const lang = explanations[st.explainLang] ? st.explainLang : Object.keys(explanations)[0] || st.explainLang;
    st.explainLang = lang;
    const grade = report.grade, pct = Math.round(report.consistency_score * 100);
    const held = report.tests.filter((x) => x.status === "ok").length, counted = report.tests.filter((x) => x.status !== "error").length;
    const ex = explanations[lang];
    const flags = report.flags.length ? report.flags.map((f) => html`<div class="card" style="padding-top:.8rem"><p><span class="mk ${f.severity === "high" ? "broke" : "wobble"}" style="display:inline">${mark(f.severity === "high" ? "broke" : "wobble", 16)}</span> <b style="font-weight:560">${f.title}</b></p><p class="muted">${f.detail}</p></div>`) : html`<p class="muted">${t("flags.none")}</p>`;
    const byCat = {};
    report.tests.forEach((x) => { (byCat[x.category] ||= []).push(x); });
    mount(host, html`
      <div class="bench-head"><h2>${t("a.title")}</h2><button class="btn line" id="again" type="button">${t("a.again")}</button></div>
      <div class="result">
        <aside>
          <div><b>${pct}<span class="hint" style="font-size:.9rem"> / 100</span></b><span>${t("explain.score") === "explain.score" ? "consistency" : t("explain.score")}</span></div>
          <div><b>${report.total_perturbations}</b><span>${t("stat.tests")}</span></div>
          <div><b>${report.unexpected_changes}</b><span>${t("stat.unexpected")}</span></div>
          ${report.seconds ? html`<div><b>${report.seconds}s</b><span>${t("stat.time")}</span></div>` : ""}
          ${report.cost_inr ? html`<div><b>${formatInr(report.cost_inr)}</b><span>${t("stat.cost")}${report.calls ? ` · ${report.calls.target + report.calls.helpers} ${t("a.calls")}` : ""}</span></div>` : ""}
          <div class="hint">${report.target_label}</div>
        </aside>
        <div class="stack" style="gap:2rem">
          <div>
            <h3 class="verdict">${t("grade." + grade)}. ${t("a.heldof", { n: held, m: counted })}</h3>
            <div class="marks" role="img" aria-label="${held}/${counted}">${report.tests.map((x) => html`<span title="${x.label}">${mark(x.status, 22)}</span>`)}</div>
            <p class="muted" lang="${report.prompt_language}" dir="auto">“${report.prompt}”</p>
          </div>
          <section class="stack"><div class="row"><h3 class="h3">${t("explain.title")}</h3><span class="grow"></span>${Object.keys(explanations).length > 1 || !isDemo ? langButton("xl") : ""}</div>
            ${ex ? html`<p class="say" id="say" lang="${lang}" dir="${dirOf(ex.text)}">${ex.sentences.map((s, i) => html`<s data-i="${i}">${s} </s>`)}</p>
              ${ex.unreviewed ? html`<p class="hint">${L(lang) ? quality(L(lang)) : t("a.mt")}. ${t("a.unrev")}</p>` : ""}${ex.source === "template" ? html`<p class="hint">${t("explain.template")}</p>` : ""}
              <div class="row">${L(lang)?.voice ? html`<button class="btn sm" id="listen" type="button">${icon("play", 16)} ${t("a.listen")}</button>` : ""}<button class="btn line" id="copy" type="button">${t("a.copy")}</button>${auth.user() && !isDemo ? html`<button class="btn line" id="save" type="button">${t("a.save")}</button>` : ""}<span class="hint" id="voice-note"></span></div>`
              : html`<p class="muted">…</p>`}
          </section>
          <section class="stack"><h3 class="h3">${t("flags.title")}</h3><div class="grid2">${flags}</div></section>
          <section class="stack" style="gap:.7rem">${report.groups.map((g) => html`<div class="bar-row"><span>${t("grp." + g.key)}</span><div class="meter"><i style="width:${Math.round(g.score * 100)}%"></i></div><b>${Math.round(g.score * 100)}</b></div>`)}</section>
          ${report.units.length ? html`<section class="stack" style="gap:.5rem"><h3 class="h3">${t("words.title")}</h3><p class="say heat" lang="${report.prompt_language}" dir="auto">${report.units.map((u) => html`<span style="--w:${u.importance}">${u.text}</span> `)}</p><p class="hint">${t("words.hint")}</p></section>` : ""}
          <section class="stack" style="gap:.6rem"><h3 class="h3">${t("tried.title")}</h3><div class="tests">${Object.entries(byCat).map(([c, xs]) => html`<details class="test"><summary><span>${t("cat." + c) === "cat." + c ? c : t("cat." + c)} <span class="hint">· ${xs.length}</span></span><span class="marks" style="margin:0">${xs.map((x) => mark(x.status, 16))}</span></summary>
            <div class="body">${xs.map((x) => html`<div class="stack" style="gap:.4rem"><span class="hint">${mark(x.status, 14)} ${x.label}</span><q class="diff" dir="auto">${highlightChanges(report.prompt, x.input)}</q>${x.output ? html`<q dir="auto">${x.output}</q>` : ""}${x.reason ? html`<span class="hint">${t("detail.why")}: ${x.reason}</span>` : ""}</div>`)}</div></details>`)}</div></section>
          <section class="grid2"><div class="stack" style="gap:.5rem"><h4 class="h3" style="font-size:1.1rem">${t("untested.title")}</h4><ul class="muted" style="margin:0;padding-inline-start:1.1rem">${report.not_tested.map((n) => html`<li>${n}</li>`)}</ul></div>
            <div class="stack" style="gap:.5rem"><h4 class="h3" style="font-size:1.1rem">${t("plan.next") === "plan.next" ? "Next steps" : t("plan.next")}</h4><ul class="muted" style="margin:0;padding-inline-start:1.1rem">${report.next_steps.map((n) => html`<li>${n}</li>`)}</ul></div></section>
          ${report.notes.length ? html`<p class="hint">${report.notes.join(" ")}</p>` : ""}
        </div>
      </div>`);
    if (grade === "steady" && !isDemo) sfx("done"); else sfx("tap");
    if (!isDemo) S.earn(10);
    $("#again", host).addEventListener("click", () => ask());
    $("#xl", host)?.addEventListener("click", (ev) => pickExplain(ev.currentTarget, async (c) => {
      if (explanations[c]) return result(report, explanations, sig, isDemo);
      if (!sig) { toast(t("a.demoonly")); st.explainLang = lang; return; }
      toast(t("explain.preparing"));
      try { explanations[c] = await api("/api/explain", { method: "POST", body: { report, sig, language: c } }); result(report, explanations, sig, isDemo); }
      catch (err) { toast(message(err)); st.explainLang = lang; }
    }));
    if (ex) {
      const sentences = $$("#say s", host);
      $("#copy", host).addEventListener("click", async () => { try { await navigator.clipboard.writeText(`${report.prompt}\n${pct}/100 ${t("grade." + grade)}\n\n${ex.text}`); toast(t("a.copied")); } catch { /* clipboard blocked */ } });
      $("#save", host)?.addEventListener("click", async () => { try { await auth.saveAudit({ prompt: report.prompt, score: report.consistency_score, grade, report, explanation: ex, lang }); toast(t("a.saved")); } catch (e) { toast(e.message); } });
      const listen = $("#listen", host), note = $("#voice-note", host);
      if (listen) {
        const total = ex.sentences.reduce((a, s) => a + s.length, 0);
        const label = (playing) => { mount(listen, html`${icon(playing ? "pause" : "play", 16)} ${playing ? t("a.pause") : t("a.listen")}`); };
        st.player = new VoicePlayer({
          onTime: (cur, dur) => { if (!dur) return; const at = (cur / dur) * total; let acc = 0, idx = 0; ex.sentences.forEach((s, i) => { if (at >= acc) idx = i; acc += s.length; }); sentences.forEach((s, i) => s.classList.toggle("now", i === idx)); },
          onState: (s) => { label(s === "playing"); if (s === "ended") sentences.forEach((x) => x.classList.remove("now")); },
        });
        listen.addEventListener("click", async () => {
          if (st.player.playing) return st.player.pause();
          try { note.textContent = t("explain.preparing"); await st.player.load(await speak(ex.text.slice(0, 1700), lang, "female")); note.textContent = ""; await st.player.play(); }
          catch (e) { note.textContent = e.message === "network" ? t("explain.voiceFail") : message(e); }
        });
      }
    }
  }

  ask();
  return cleanup;
}
