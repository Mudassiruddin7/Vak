// Step four: the score, the explanation in plain Telugu or Hindi (with a voice), and the evidence behind it.
import { $, $$, html, mount, formatInr, raw } from "../dom.js";
import { icon } from "../icons.js";
import { t, getLang } from "../i18n.js";
import { api } from "../api.js";
import { confetti, countUp } from "../fx.js";
import { VoicePlayer, fmtTime, loudness } from "../player.js";
import { on, state } from "../state.js";
import { tryCard, wireCards } from "./shared.js";

const CIRC = 2 * Math.PI * 76;
const BARS = 28;

function tick(p) {
  const a = p * Math.PI * 2;
  const pt = (r) => [(100 + r * Math.cos(a)).toFixed(1), (100 + r * Math.sin(a)).toFixed(1)];
  const [x1, y1] = pt(66);
  const [x2, y2] = pt(86);
  return raw(`<line class="tick" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`);
}

export function renderResult(ctx) {
  const { panel, mascot, cfg } = ctx;
  const run = state.run;
  const report = run.report;
  const promptLang = report.prompt_language;
  const grade = report.grade;
  const pct = Math.round(report.consistency_score * 100);
  const sample = state.sample;
  const opts = state.opts;
  const explainLang = () => run.explainLang;

  // flags, steps and "not tested" come from the engine in English; for Telugu and Hindi we swap in the
  // hand-checked wording by code. Old audits saved before the codes existed simply stay in English.
  const bank = state.uiLang !== "en" ? cfg.findings?.[state.uiLang] : null;
  const flagTitle = (f) => bank?.flags[f.code] ?? f.title;
  const stepList = bank && report.next_step_codes?.length
    ? report.next_step_codes.map((c, i) => bank.steps[c] ?? report.next_steps[i]) : report.next_steps;
  const untestedList = bank && report.not_tested_codes?.length
    ? report.not_tested_codes.map((c, i) => bank.not_tested[c] ?? report.not_tested[i]) : report.not_tested;
  const strengthList = state.uiLang === "en" ? report.strengths
    : report.groups.filter((g) => g.score >= 0.9).slice(0, 4).map((g) => `${t(`grp.${g.key}`)}: ${t("strength.held", { n: g.n })}`);

  const stats = [
    [report.total_perturbations, t("stat.tests")],
    [report.unexpected_changes, t("stat.unexpected")],
    [Math.round(report.seconds || 0), t("stat.time")],
    [formatInr(report.cost_inr || 0), t("stat.cost")],
  ];
  const topUnit = report.units.length ? Math.max(...report.units.map((u) => u.importance)) : 0;
  const flat = report.units.length > 0 && topUnit === 0;
  const faith = report.faithfulness;
  const faithText = faith.status === "ok"
    ? t(faith.score >= 0.7 ? "faith.good" : faith.score >= 0.4 ? "faith.partial" : "faith.poor")
    : t(`faith.${faith.status}`);

  mount(panel, html`
    ${sample ? html`<div class="callout view-in" style="margin-bottom:16px"><b>${t("sample.badge")}.</b> ${t("sample.note")}</div>` : ""}

    <section class="card score-card view-in" id="score-card">
      <div class="gauge" data-grade="${grade}" role="img" aria-label="${t("score.label")}: ${pct} ${t("score.outOf")}">
        <svg viewBox="0 0 200 200" aria-hidden="true">
          <circle class="track" cx="100" cy="100" r="76"/>
          <circle class="arc" id="arc" cx="100" cy="100" r="76"/>
          ${tick(0.6)}${tick(0.85)}
        </svg>
        <div class="num"><b id="score-num">0</b><span>${t("score.outOf")}</span></div>
      </div>
      <div>
        <span class="verdict" data-grade="${grade}">${t(`grade.${grade}`)}</span>
        <h2>${t("score.label")}</h2>
        <p class="target-line">${report.target_label} · ${t(`depth.${report.depth}`)} · <span lang="${promptLang}">“${report.prompt}”</span></p>
        <div class="stats">${stats.map(([n, label]) => html`<div class="stat"><b>${n}</b><span>${label}</span></div>`)}</div>
      </div>
    </section>

    <article class="card explain view-in" id="explain-card">
      <div class="card-title">
        <h2>${t("explain.title")}</h2>
        <div class="segmented" id="e-lang" role="group">
          <button type="button" data-l="te" aria-pressed="${explainLang() === "te"}">తెలుగు</button>
          <button type="button" data-l="hi" aria-pressed="${explainLang() === "hi"}">हिन्दी</button>
        </div>
      </div>
      <div id="e-body"></div>
      <div class="player" id="player" ${sample ? "hidden" : ""}>
        <button type="button" class="play" id="play" aria-label="${t("explain.listen")}">${icon("play")}</button>
        <div class="viz" id="viz" aria-hidden="true">${raw("<i></i>".repeat(BARS))}</div>
        <span class="time" id="time">0:00</span>
        <select id="voice" aria-label="${t("explain.voice")}"></select>
        <select id="speed" aria-label="${t("explain.speed")}">
          <option value="0.85">0.85×</option><option value="1" selected>1×</option><option value="1.15">1.15×</option>
        </select>
        <input type="range" class="scrub" id="scrub" min="0" max="1000" value="0" aria-label="Seek">
        <span class="cost-note" id="cost-note"></span>
      </div>
      <div class="e-tools" style="margin-top:16px">
        <button type="button" class="btn btn-ghost btn-small" id="copy">${icon("copy")} ${t("explain.copy")}</button>
        ${sample ? "" : html`<a class="btn btn-ghost btn-small" href="/api/audits/${run.id}/report.md" download>${icon("download")} ${t("explain.report")}</a>`}
      </div>
      <p class="e-source" id="e-source"></p>
    </article>

    <section class="card view-in" id="tried-card">
      <div class="card-title"><h2>${t("tried.title")}</h2></div>
      <div class="groups">
        ${report.groups.map((g, i) => html`
          <div class="group" data-grade="${g.score < 0.6 ? "concerning" : g.score < 0.85 ? "mixed" : "steady"}" style="--n:${i}">
            <span>${t(`grp.${g.key}`)}</span>
            <div class="g-bar"><i data-w="${Math.round(g.score * 100)}"></i></div>
            <span class="g-n">${Math.round(g.score * 100)}%</span>
          </div>`)}
      </div>
      <button type="button" class="btn btn-ghost btn-small" id="toggle-tries" aria-expanded="false">${t("tried.show")}</button>
      <div class="try-grid" id="tries" hidden></div>
    </section>

    <div class="two-up">
      <section class="card view-in" id="words-card">
        <div class="card-title"><h2>${t("words.title")}</h2></div>
        ${!report.units.length
          ? html`<p class="soft">${t("words.skip")}</p>`
          : html`
            <p class="heat" lang="${promptLang}">${report.units.map((u, i) => html`<span class="w" style="--i:${u.importance};--k:${i}" data-tip="${Math.round(u.importance * 100)}%" ${u.importance >= 0.99 && u.raw >= 0.4 ? raw('data-top="1"') : ""}>${u.text}</span> `)}</p>
            ${flat ? html`<div class="callout good" style="margin-top:14px">${t("words.none")}</div>` : html`<div class="legend"><span>0%</span><span class="ramp"></span><span>100%</span></div><p class="muted" style="margin-top:10px;font-size:.86rem">${t("words.hint")}</p>`}`}
      </section>

      <section class="card view-in" id="faith-card">
        <div class="card-title"><h2>${t("faith.title")}</h2></div>
        <p>${faithText}</p>
        ${faith.status === "ok" ? html`<div class="faith-meter"><i data-w="${Math.round((faith.score ?? 0) * 100)}"></i></div>` : ""}
        ${faith.claimed.length ? html`<p class="muted" style="font-size:.86rem;margin-top:8px">${t("faith.said")}: ${faith.claimed.map((c) => html`<span class="chip" style="padding:2px 10px;margin:2px;cursor:default" lang="${promptLang}">${c}</span>`)}</p>` : ""}
      </section>
    </div>

    <div class="two-up">
      <section class="card view-in" id="flags-card">
        <div class="card-title"><h2>${t("flags.title")}</h2></div>
        ${report.flags.length
          ? report.flags.map((f) => html`<div class="flag" data-sev="${f.severity}"><span class="sev"></span><b>${flagTitle(f)}</b><p>${f.detail}</p></div>`)
          : html`<div class="callout good">${t("flags.none")}</div>`}
        ${strengthList.length ? html`<h3 style="margin:18px 0 10px">${t("strengths.title")}</h3><ul class="bullets">${strengthList.map((s) => html`<li>${s}</li>`)}</ul>` : ""}
      </section>

      <section class="card view-in" id="steps-card">
        <div class="card-title"><h2>${t("steps.title")}</h2></div>
        <ul class="bullets">${stepList.map((s) => html`<li>${s}</li>`)}</ul>
        <h3 style="margin:20px 0 10px">${t("untested.title")}</h3>
        <ul class="bullets dim">${untestedList.map((s) => html`<li>${s}</li>`)}</ul>
      </section>
    </div>

    ${report.notes.length ? html`<section class="card view-in"><div class="card-title"><h3>${t("notes.title")}</h3></div><ul class="bullets dim">${report.notes.map((n) => html`<li>${n}</li>`)}</ul></section>` : ""}

    <div class="result-actions">
      <button type="button" class="btn btn-primary" id="again">${icon("plus")} ${t("again")}</button>
      ${run.plan ? html`<button type="button" class="btn btn-ghost" id="rerun">${icon("refresh")} ${t("rerun")}</button>` : ""}
    </div>`);

  // --- the gauge ------------------------------------------------------------------------------
  const arc = $("#arc", panel);
  arc.style.strokeDasharray = String(CIRC);
  arc.style.strokeDashoffset = String(CIRC);
  requestAnimationFrame(() => requestAnimationFrame(() => {
    arc.style.strokeDashoffset = String(CIRC * (1 - report.consistency_score));
    countUp($("#score-num", panel), pct, { ms: 1500 });
    $$("[data-w]", panel).forEach((bar) => { bar.style.width = `${bar.dataset.w}%`; });
  }));

  // --- mood -----------------------------------------------------------------------------------
  const verdictMood = grade === "steady" ? "happy" : grade === "concerning" ? "worried" : "mixed";
  const sayVerdict = () => mascot.say(t(grade === "steady" ? "bubble.happy" : grade === "concerning" ? "bubble.worried" : "bubble.mixed"));
  mascot.setMood(verdictMood);
  sayVerdict();
  if (grade === "steady" && !run.celebrated) { run.celebrated = true; setTimeout(() => confetti(), 700); }

  // --- every try -----------------------------------------------------------------------------
  const tries = $("#tries", panel);
  const toggle = $("#toggle-tries", panel);
  toggle.addEventListener("click", () => {
    const open = tries.hidden;
    if (open && !tries.children.length) {
      tries.insertAdjacentHTML("beforeend", report.tests.map((test, i) => tryCard(test, i, report.prompt, promptLang).markup).join(""));
      wireCards(tries);
    }
    tries.hidden = !open;
    toggle.setAttribute("aria-expanded", String(open));
    toggle.textContent = open ? t("tried.hide") : t("tried.show");
  });

  // --- the explanation ---------------------------------------------------------------------------
  const body = $("#e-body", panel);
  const sourceNote = $("#e-source", panel);
  const playBtn = $("#play", panel);
  const bars = $$("#viz i", panel);
  const time = $("#time", panel);
  const scrub = $("#scrub", panel);
  const costNote = $("#cost-note", panel);
  const voiceSel = $("#voice", panel);
  const speedSel = $("#speed", panel);
  let spans = [];
  let bounds = [];
  let nowIndex = -1;

  const player = new VoicePlayer({
    onTime(cur, dur) {
      time.textContent = fmtTime(cur);
      if (dur) scrub.value = String(Math.round((cur / dur) * 1000));
      highlight(dur ? cur / dur : 0);
    },
    onState(st) {
      const playing = st === "playing";
      playBtn.replaceChildren();
      playBtn.insertAdjacentHTML("beforeend", icon(playing ? "pause" : "play").markup);
      playBtn.setAttribute("aria-label", playing ? t("explain.pause") : t("explain.listen"));
      mascot.setMood(playing ? "speaking" : verdictMood);
      mascot.setAudioDriven(true);
      if (playing) mascot.say(t("bubble.speaking")); else { sayVerdict(); highlight(-1); }
    },
    onFrame(bins) {
      mascot.mouth(loudness(bins));
      bars.forEach((bar, i) => {
        const v = bins && bins.length ? bins[Math.min(bins.length - 1, 1 + Math.floor((i / BARS) * bins.length * 0.7))] / 255 : 0;
        bar.style.transform = `scaleY(${Math.max(0.12, Math.min(1, v * 1.3)).toFixed(3)})`;
      });
    },
  });

  function highlight(ratio) {
    if (!spans.length) return;
    let idx = -1;
    if (ratio >= 0 && player.playing) idx = Math.max(0, bounds.findIndex((b) => ratio <= b));
    if (idx === nowIndex) return;
    spans[nowIndex]?.classList.remove("is-now");
    spans[idx]?.classList.add("is-now");
    nowIndex = idx;
  }

  function paintVoices() {
    const voices = cfg.voices[explainLang()] || [];
    voiceSel.replaceChildren();
    voices.forEach((v, i) => {
      const o = document.createElement("option");
      o.value = v.id;
      o.textContent = `${v.name} · ${v.gender === "F" ? "♀" : "♂"}`;
      if (v.id === opts.voiceId || (!opts.voiceId && i === 0)) o.selected = true;
      voiceSel.append(o);
    });
  }

  function paintExplanation(animate = false) {
    player.pause();
    const lang = explainLang();
    const ex = run.explanations[lang];
    $$("#e-lang button", panel).forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.l === lang)));
    paintVoices();
    if (!ex) {
      body.innerHTML = '<div class="skeleton-lines" style="position:relative;inset:auto;padding:6px 0"><i></i><i></i><i></i><i></i></div>';
      sourceNote.textContent = t("phase.explain");
      spans = []; bounds = [];
      return;
    }
    sayVerdict();
    const total = ex.sentences.reduce((n, s) => n + s.length, 0) || 1;
    let acc = 0;
    bounds = ex.sentences.map((s) => (acc += s.length) / total);
    body.replaceChildren();
    mount(body, html`<p class="read ${animate ? "is-new" : ""}" lang="${lang}" id="read">${ex.sentences.map((s, i) => html`<span class="s" style="--k:${i}">${s}</span> `)}</p>`);
    spans = $$("#read .s", body);
    nowIndex = -1;
    sourceNote.textContent = ex.source === "template" ? t("explain.template") : "";
    const audio = run.audio[lang];
    costNote.textContent = "";
    if (audio) player.load(audio.url).catch(() => { delete run.audio[lang]; });
    time.textContent = "0:00";
    scrub.value = "0";
  }

  async function play() {
    if (player.playing) { player.pause(); return; }
    const lang = explainLang();
    let info = run.audio[lang];
    if (!info) {
      playBtn.classList.add("is-loading");
      playBtn.replaceChildren();
      playBtn.insertAdjacentHTML("beforeend", icon("loader").markup);
      costNote.textContent = t("explain.preparing");
      mascot.setMood("thinking");
      try {
        info = await api.speak(run.id, { language: lang, voice_id: voiceSel.value || null, pace: 1 });
        run.audio[lang] = info;
        costNote.textContent = info.cost_inr ? t("explain.cost", { n: formatInr(info.cost_inr) }) : "";
        ctx.refreshBudget(true);
      } catch (err) {
        costNote.textContent = "";
        ctx.toast(err.message || t("explain.voiceFail"), "bad");
        return;
      } finally {
        playBtn.classList.remove("is-loading");
        playBtn.replaceChildren();
        playBtn.insertAdjacentHTML("beforeend", icon("play").markup);
      }
    }
    try {
      await player.load(info.url);
      player.setRate(Number(speedSel.value));
      await player.play();
    } catch {
      ctx.toast(t("explain.voiceFail"), "bad");
    }
  }

  playBtn.addEventListener("click", play);
  speedSel.addEventListener("change", () => player.setRate(Number(speedSel.value)));
  voiceSel.addEventListener("change", () => {
    opts.voiceId = voiceSel.value;
    delete run.audio[explainLang()];        // a different voice needs a fresh recording
    costNote.textContent = "";
    player.pause();
  });
  scrub.addEventListener("input", () => player.seek(Number(scrub.value) / 1000));

  $("#e-lang", panel).addEventListener("click", async (event) => {
    const b = event.target.closest("button[data-l]");
    if (!b || b.dataset.l === explainLang()) return;
    run.explainLang = b.dataset.l;
    paintExplanation();
    if (!run.explanations[run.explainLang]) {
      try {
        run.explanations[run.explainLang] = await api.explain(run.id, run.explainLang);
        ctx.refreshBudget(true);
        paintExplanation(true);
      } catch (err) {
        ctx.toast(err.message || t("err.generic"), "bad");
      }
    }
  });

  $("#copy", panel).addEventListener("click", async (event) => {
    const ex = run.explanations[explainLang()];
    if (!ex) return;
    try {
      await navigator.clipboard.writeText(ex.text);
      const btn = event.currentTarget;
      const old = btn.innerHTML;
      btn.textContent = t("explain.copied");
      setTimeout(() => { btn.innerHTML = old; }, 1400);
    } catch { /* clipboard may be blocked; nothing to do */ }
  });

  $("#again", panel).addEventListener("click", () => ctx.reset());
  $("#rerun", panel)?.addEventListener("click", () => { state.plan = structuredClone(run.plan); ctx.go("plan"); });

  paintExplanation(true);

  const off = [
    on("explanation", (ex) => {
      run.explanations[ex.language] = ex;
      if (ex.language === explainLang()) paintExplanation(true);
    }),
    on("audio", (info) => {
      run.audio[info.language] = info;
      if (info.language === explainLang()) player.load(info.url).catch(() => {});
    }),
    on("notice", (d) => ctx.toast(d.message)),
    on("failure", (d) => ctx.toast(d.message, "bad")),
  ];

  return {
    dispose() {
      off.forEach((f) => f());
      player.destroy();
      mascot.setMood("idle");
    },
  };
}
