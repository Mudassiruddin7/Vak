// Step two: show what the orchestrator understood and let the person adjust it before spending anything.
import { $, $$, html, mount, debounce, formatInr } from "../dom.js";
import { icon } from "../icons.js";
import { t } from "../i18n.js";
import { api } from "../api.js";
import { state } from "../state.js";

const CHECKS = [
  { id: "consistency", icon: "resample" },
  { id: "perturbation", icon: "typo" },
  { id: "behavioral", icon: "negation" },
];

function targetValue(target) {
  return target.kind === "custom" ? "custom" : `${target.kind}:${target.model}`;
}

export function renderPlan(ctx) {
  const { panel, mascot, cfg } = ctx;
  const plan = state.plan;
  const opts = state.opts;
  opts.depth = plan.depth;
  opts.types = [...plan.audit_types];
  const explainLang = opts.explainLang;
  const promptLang = plan.prompt_language === "en" ? "en" : plan.prompt_language;

  const price = (depth) => state.estimates?.[depth]?.total_inr ?? 0;
  const calls = (depth) => state.estimates?.[depth]?.calls ?? 0;

  mount(panel, html`
    <section class="card view-in" id="plan-card">
      <div class="plan-head">
        <h2>${t("plan.title")}</h2>
        ${plan.source_text && plan.source_text !== plan.prompt
          ? html`<p class="heard">${icon("mic", 'style="width:16px;height:16px;flex:none;margin-top:3px"')}<span>${t("ask.heard")}: <q lang="${promptLang}">${plan.source_text}</q></span></p>`
          : ""}
      </div>

      <div class="plan-grid">
        <div>
          <label for="plan-prompt">${t("plan.prompt")}</label>
          <textarea id="plan-prompt" lang="${promptLang}" rows="3" maxlength="1500">${plan.prompt}</textarea>
        </div>

        <div class="row two">
          <div>
            <label for="plan-target">${t("plan.model")}</label>
            <select id="plan-target">
              <optgroup label="${t("plan.onSarvam")}">
                ${cfg.targets.sarvam.map((m) => html`<option value="sarvam:${m.id}" ${targetValue(plan.target) === `sarvam:${m.id}` ? "selected" : ""}>${m.label}</option>`)}
              </optgroup>
              <optgroup label="${t("plan.practice")}">
                ${cfg.targets.demo.map((m) => html`<option value="demo:${m.id}" ${targetValue(plan.target) === `demo:${m.id}` ? "selected" : ""}>${m.label}</option>`)}
              </optgroup>
              ${cfg.targets.custom_allowed ? html`<optgroup label="${t("plan.custom")}"><option value="custom" ${plan.target.kind === "custom" ? "selected" : ""}>${t("plan.custom")}</option></optgroup>` : ""}
            </select>
          </div>
          <div>
            <label>${t("plan.explainIn")}</label>
            <div class="segmented" id="explain-lang" role="group">
              <button type="button" data-l="te" aria-pressed="${explainLang === "te"}">తెలుగు</button>
              <button type="button" data-l="hi" aria-pressed="${explainLang === "hi"}">हिन्दी</button>
            </div>
          </div>
        </div>

        <div class="custom-target" id="custom-target" ${plan.target.kind === "custom" ? "" : "hidden"}>
          <div><label for="ct-url">${t("plan.endpoint")}</label><input type="url" id="ct-url" placeholder="http://localhost:11434/v1" value="${plan.target.base_url || ""}"></div>
          <div><label for="ct-model">${t("plan.endpointModel")}</label><input type="text" id="ct-model" value="${plan.target.remote_model || ""}"></div>
          <div><label for="ct-key">${t("plan.endpointKey")}</label><input type="password" id="ct-key" autocomplete="off" value="${plan.target.api_key || ""}"></div>
        </div>

        <div>
          <label>${t("plan.checks")}</label>
          <div class="check-chips" id="checks">
            ${CHECKS.map((c) => html`<button type="button" class="chip" data-check="${c.id}" aria-pressed="${opts.types.includes(c.id)}" title="${t(`plan.checkHint.${c.id}`)}">${icon(c.icon)} ${t(`check.${c.id}`)}</button>`)}
          </div>
        </div>

        <div>
          <label>${t("plan.depth")}</label>
          <div class="depth" id="depth">
            ${["quick", "standard", "deep"].map((d) => html`
              <button type="button" data-depth="${d}" aria-pressed="${opts.depth === d}">
                <span class="d-name">${t(`depth.${d}`)}</span>
                <span class="d-price" data-price="${d}">${formatInr(price(d))}</span>
                <span class="d-meta" data-calls="${d}">${t("depth.calls", { n: calls(d) })}</span>
              </button>`)}
          </div>
        </div>

        ${plan.notes.length ? html`<div class="callout"><b>${t("plan.notes")}</b><ul>${plan.notes.map((n) => html`<li>${n}</li>`)}</ul></div>` : ""}
      </div>

      <div class="plan-foot">
        <button type="button" class="btn btn-ghost" id="plan-back">${icon("back")} ${t("plan.back")}</button>
        <span class="grow"></span>
        <label class="switch"><input type="checkbox" id="plan-voice" ${opts.voice ? "checked" : ""}><span class="track"></span>${t("plan.voice")}</label>
        <button type="button" class="btn btn-primary" id="plan-start">${t("plan.start")} · <span id="plan-cost">${formatInr(price(opts.depth))}</span> ${icon("go", 'class="go"')}</button>
      </div>
    </section>`);

  mascot.setMood("idle");
  mascot.say(t("bubble.plan"));

  const promptBox = $("#plan-prompt", panel);
  const targetSel = $("#plan-target", panel);
  const customBox = $("#custom-target", panel);
  const startBtn = $("#plan-start", panel);

  const paintPrices = () => {
    for (const d of ["quick", "standard", "deep"]) {
      $(`[data-price="${d}"]`, panel).textContent = formatInr(price(d));
      $(`[data-calls="${d}"]`, panel).textContent = t("depth.calls", { n: calls(d) });
    }
    $("#plan-cost", panel).textContent = formatInr(price(opts.depth));
  };

  const refreshEstimates = debounce(async () => {
    try {
      const out = await api.estimate({ ...state.plan, audit_types: opts.types, depth: opts.depth }, opts.voice);
      state.estimates = out.estimates;
      paintPrices();
    } catch { /* the old numbers are still a fair guide */ }
  }, 350);

  promptBox.addEventListener("input", () => { plan.prompt = promptBox.value; refreshEstimates(); });

  targetSel.addEventListener("change", () => {
    const [kind, ...rest] = targetSel.value.split(":");
    const keep = plan.target.system_prompt;
    plan.target = kind === "custom"
      ? { kind: "custom", model: cfg.targets.default, base_url: $("#ct-url", panel).value, remote_model: $("#ct-model", panel).value, api_key: $("#ct-key", panel).value, system_prompt: keep }
      : { kind, model: rest.join(":"), system_prompt: keep };
    customBox.hidden = kind !== "custom";
    refreshEstimates();
  });
  for (const id of ["ct-url", "ct-model", "ct-key"]) {
    $(`#${id}`, panel).addEventListener("input", (e) => {
      const field = { "ct-url": "base_url", "ct-model": "remote_model", "ct-key": "api_key" }[id];
      plan.target[field] = e.target.value;
    });
  }

  $("#explain-lang", panel).addEventListener("click", (e) => {
    const b = e.target.closest("button[data-l]");
    if (!b) return;
    opts.explainLang = b.dataset.l;
    $$("#explain-lang button", panel).forEach((x) => x.setAttribute("aria-pressed", String(x === b)));
  });

  $("#checks", panel).addEventListener("click", (e) => {
    const chip = e.target.closest("[data-check]");
    if (!chip) return;
    const id = chip.dataset.check;
    const on = opts.types.includes(id);
    if (on && opts.types.length === 1) return;           // at least one check has to stay on
    opts.types = on ? opts.types.filter((x) => x !== id) : [...opts.types, id];
    chip.setAttribute("aria-pressed", String(!on));
    refreshEstimates();
  });

  $("#depth", panel).addEventListener("click", (e) => {
    const b = e.target.closest("[data-depth]");
    if (!b) return;
    opts.depth = b.dataset.depth;
    $$("#depth button", panel).forEach((x) => x.setAttribute("aria-pressed", String(x === b)));
    $("#plan-cost", panel).textContent = formatInr(price(opts.depth));
  });

  $("#plan-voice", panel).addEventListener("change", (e) => { opts.voice = e.target.checked; refreshEstimates(); });
  $("#plan-back", panel).addEventListener("click", () => { ctx.pipeline("orchestrator", "idle"); ctx.go("ask"); });

  startBtn.addEventListener("click", async () => {
    plan.prompt = promptBox.value.trim();
    if (!plan.prompt) { promptBox.focus(); return; }
    startBtn.disabled = true;
    try {
      await ctx.startAudit({
        plan: { ...plan, audit_types: opts.types, depth: opts.depth },
        language: opts.explainLang,
        voice: opts.voice,
        voice_id: opts.voiceId,
        fresh: state.adv.fresh,
      });
    } catch (err) {
      startBtn.disabled = false;
      mascot.setMood("worried");
      setTimeout(() => mascot.setMood("idle"), 2200);
      ctx.toast(err.message || t("err.generic"), "bad");
    }
  });

  return { dispose() {} };
}
