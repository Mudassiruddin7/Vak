// Starts the app, owns the view switching, and turns the server's event stream into state.
import { $, $$, el, formatInr, html, mount } from "./dom.js";
import { icon } from "./icons.js";
import { MITTHU_MARK } from "./mascot-svg.js";
import { createMascot } from "./mascot.js";
import { api, ApiError, listen } from "./api.js";
import { getLang, initialLang, setLang, t } from "./i18n.js";
import { apply, refreshLabels, renderPipeline } from "./pipeline.js";
import { emit, freshRun, runFromRecord, state, STAGES } from "./state.js";
import { followPointerGlow, glyphField } from "./fx.js";
import { openBudget, openHistory } from "./drawers.js";
import { renderAsk } from "./views/ask.js";
import { renderPlan } from "./views/plan.js";
import { renderRun } from "./views/run.js";
import { renderResult } from "./views/result.js";

const VIEWS = { ask: renderAsk, plan: renderPlan, run: renderRun, result: renderResult };
const app = $("#app");
const panel = $("#panel");
const pipelineHost = $("#pipeline");
let current = null;
let stages = Object.fromEntries(STAGES.map((s) => [s, "idle"]));

const ctx = { panel, state, cfg: null, mascot: null, go, reset, toast, pipeline, refreshBudget, startAudit, openRecord, openExample };

// --- pipeline strip --------------------------------------------------------------------------
function pipeline(name, status) {
  stages[name] = status;
  apply(pipelineHost, stages);
}
function setStages(next) {
  stages = { ...Object.fromEntries(STAGES.map((s) => [s, "idle"])), ...next };
  apply(pipelineHost, stages);
}

// --- views ----------------------------------------------------------------------------------------
function go(view, { scroll = true } = {}) {
  current?.dispose?.();
  state.view = view;
  app.dataset.view = view;
  current = VIEWS[view](ctx);
  if (view === "ask") setStages({});
  else if (view === "plan") setStages({ input: "done", orchestrator: "done" });
  if (scroll) window.scrollTo({ top: 0, behavior: "smooth" });
}

function reset() {
  state.run?.close?.();
  state.run = null;
  state.plan = null;
  state.sample = false;
  state.askText = "";
  state.heard = "";
  state.overrides = {};
  go("ask");
}

// --- toasts -----------------------------------------------------------------------------------------
function toast(message, kind = "") {
  const node = el("div", { class: `toast ${kind}`, role: "status" }, message);
  $("#toasts").append(node);
  setTimeout(() => { node.classList.add("out"); setTimeout(() => node.remove(), 320); }, 4200);
}

// --- budget pill ---------------------------------------------------------------------------------
let lastRemaining = null;
async function refreshBudget(animate = false) {
  try {
    const b = await api.budget();
    state.budget = b;
    const label = $("#budget-left");
    label.textContent = formatInr(b.total.remaining);
    if (animate && lastRemaining != null && b.total.remaining < lastRemaining - 0.0001) {
      const coin = $("#coin");
      coin.classList.remove("plink");
      void coin.offsetWidth;
      coin.classList.add("plink");
    }
    lastRemaining = b.total.remaining;
  } catch { /* the pill just keeps its last value */ }
}

// --- running an audit ------------------------------------------------------------------------------------
async function startAudit(request) {
  const { id } = await api.start(request);
  const run = freshRun(id);
  run.plan = structuredClone(request.plan);
  if (run.plan.target) delete run.plan.target.api_key;
  run.explainLang = request.language;
  state.run = run;
  state.sample = false;
  setStages({ input: "done", orchestrator: "done" });
  ctx.refreshBudget(true);
  streamRun(run);
  go("run");
}

function streamRun(run) {
  const handlers = {
    queued() { run.queued = true; emit("queued"); },
    phase(d) { run.phase = d.name; emit("phase", d); },
    plan(d) {
      run.variants = [...d.variants];
      for (let i = 1; i <= d.resamples; i++) {
        run.variants.push({ id: `resample-${i}`, category: "resample", kind: "resample", label: `Asked again (try ${i})`, input: state.plan?.prompt || run.plan?.prompt || "" });
      }
      emit("plan", d);
    },
    baseline(d) { run.baseline = d.output; emit("baseline", d); },
    answer(d) { run.answers.set(d.id, d); emit("answer", d); },
    progress(d) { run.progress = d; emit("progress", d); },
    test(d) { run.tests.set(d.id, d); emit("test", d); },
    units(d) { run.units = d.units; emit("units", d); },
    report(d) { run.report = d; emit("report", d); },
    stage(d) {
      pipeline(d.name, d.state);
      run.stages = { ...stages };
      if (d.name === "explain" && d.state === "active") ctx.mascot.say(t("phase.explain"));
      emit("stage", d);
    },
    explanation(d) { run.explanations[d.language] = d; emit("explanation", d); },
    audio(d) { run.audio[d.language] = d; emit("audio", d); },
    notice(d) { emit("notice", d); },
    done(d) { run.status = "done"; ctx.refreshBudget(true); emit("done", d); },
    failure(d) {
      run.status = "failed";
      run.error = d.message;
      const busy = STAGES.find((s) => stages[s] === "active") || "engine";
      pipeline(busy, "error");
      emit("failure", d);
    },
    cancelled(d) { run.status = "cancelled"; setStages({ input: "done", orchestrator: "done" }); emit("cancelled", d); },
    lost() { toast(t("err.network"), "bad"); },
  };
  run.close = listen(run.id, handlers);
}

// --- opening finished audits (history and bundled examples) ---------------------------------------------
function showRecord(record, { sample = false } = {}) {
  state.run?.close?.();
  state.run = runFromRecord(record);
  state.run.plan = record.request?.plan || null;
  state.sample = sample;
  setStages(Object.fromEntries(STAGES.map((s) => [s, "done"])));
  go("result");
}

async function openRecord(id) {
  try {
    const record = await api.audit(id);
    if (!record.report) { toast(record.error || t("err.generic"), "bad"); return; }
    showRecord(record);
  } catch (err) {
    toast(err.message, "bad");
  }
}

async function openExample(name) {
  try {
    const res = await fetch(`/demo/${name}.json`);
    if (!res.ok) throw new Error("example missing");
    const record = await res.json();
    record.explanations && Object.values(record.explanations).forEach((e) => { delete e.audio; });
    showRecord(record, { sample: true });
  } catch {
    toast(t("err.generic"), "bad");
  }
}

// --- chrome: language, theme, drawers --------------------------------------------------------------
function applyChrome() {
  $$("[data-i18n]").forEach((node) => { node.textContent = t(node.dataset.i18n); });
  $$("[data-i18n-html]").forEach((node) => { node.innerHTML = t(node.dataset.i18nHtml); });
  $$("[data-i18n-aria]").forEach((node) => { node.setAttribute("aria-label", t(node.dataset.i18nAria)); });
  const index = ["te", "hi", "en"].indexOf(getLang());
  $(".langswitch").style.setProperty("--i", String(index));
  $$(".langswitch button").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.lang === getLang())));
  refreshLabels(pipelineHost);
}

function changeLang(code) {
  setLang(code);
  state.uiLang = code;
  applyChrome();
  go(state.view, { scroll: false });
}

function applyTheme(theme) {
  if (theme) document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
  const dark = (theme || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")) === "dark";
  $("#btn-theme").innerHTML = icon(dark ? "sun" : "moon").markup;
}

function wireChrome() {
  $(".brand .brand-mark-slot").innerHTML = MITTHU_MARK;
  $$(".langswitch button").forEach((b) => b.addEventListener("click", () => changeLang(b.dataset.lang)));
  $("#btn-budget").addEventListener("click", () => openBudget());
  $("#btn-history").addEventListener("click", () => openHistory(ctx));
  $("#btn-history").innerHTML = icon("clock").markup;
  $("#coin").innerHTML = icon("coin").markup;

  let theme = null;
  try { theme = localStorage.getItem("vak.theme"); } catch { /* fine */ }
  applyTheme(theme);
  $("#btn-theme").addEventListener("click", () => {
    const isDark = document.documentElement.dataset.theme === "dark" ||
      (!document.documentElement.dataset.theme && matchMedia("(prefers-color-scheme: dark)").matches);
    theme = isDark ? "light" : "dark";
    try { localStorage.setItem("vak.theme", theme); } catch { /* fine */ }
    applyTheme(theme);
  });

  const bar = $("#topbar");
  const onScroll = () => bar.classList.toggle("scrolled", window.scrollY > 8);
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();
}

// --- boot ---------------------------------------------------------------------------------------------------
async function boot() {
  const params = new URLSearchParams(location.search);
  if (params.has("still")) document.documentElement.setAttribute("data-still", "");
  if (params.get("lang")) try { localStorage.setItem("vak.lang", params.get("lang")); } catch { /* fine */ }
  state.uiLang = setLang(initialLang());
  wireChrome();
  glyphField($(".glyph-field"));
  followPointerGlow();
  renderPipeline(pipelineHost, stages);
  ctx.mascot = createMascot($("#stage"));
  applyChrome();

  try {
    ctx.cfg = state.cfg = await api.config();
  } catch (err) {
    mount(panel, html`<div class="card"><div class="callout bad">${err instanceof ApiError ? err.message : t("err.network")}</div></div>`);
    return;
  }
  refreshBudget();

  if (params.get("demo")) {
    await openExample(params.get("demo"));
    const lang = params.get("explain");
    if (lang && state.run) { state.run.explainLang = lang; go("result", { scroll: false }); }
    return;
  }
  go("ask", { scroll: false });
}

boot();
