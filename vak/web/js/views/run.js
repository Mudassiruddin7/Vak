// Step three: watch the audit happen. Cards appear for every variation and settle as results come in.
import { $, html, mount } from "../dom.js";
import { t } from "../i18n.js";
import { api } from "../api.js";
import { on, state } from "../state.js";
import { setCardState, spinner, tryCard, wireCards } from "./shared.js";

export function renderRun(ctx) {
  const { panel, mascot } = ctx;
  const run = state.run;
  const promptLang = state.plan?.prompt_language || "te";
  const original = state.plan?.prompt || "";

  mount(panel, html`
    <section class="card view-in" id="run-card">
      <div class="run-head">
        <div class="narration" id="narration">${spinner()}<span id="narr-text">${t("phase.variants")}</span></div>
        <div class="progress">
          <div class="bar"><i id="bar"></i></div>
          <span class="count" id="count"></span>
          <button type="button" class="btn btn-ghost btn-small" id="cancel">${t("run.cancel")}</button>
        </div>
      </div>
      <div class="baseline" id="baseline" hidden>
        <div class="lbl">${t("run.baseline")}</div>
        <p></p>
      </div>
      <h3 style="margin-top:22px">${t("run.tries")}</h3>
      <div class="try-grid" id="grid"></div>
    </section>`);

  const grid = $("#grid", panel);
  const narration = $("#narr-text", panel);
  const bar = $("#bar", panel);
  const count = $("#count", panel);
  wireCards(grid);

  const say = (key) => { narration.textContent = t(key); };

  function paintProgress() {
    const { done, total } = run.progress;
    bar.style.width = total ? `${Math.round((done / total) * 100)}%` : "4%";
    count.textContent = total ? t("run.progress", { done, total }) : "";
  }

  function paintCards() {
    grid.replaceChildren();
    const specs = run.variants.length ? run.variants : [];
    const html_ = specs.map((v, i) => {
      const done = run.tests.get(v.id);
      const answered = run.answers.get(v.id);
      const status = done ? done.status : answered ? (answered.error ? "error" : "answered") : "waiting";
      return tryCard({ ...v, ...(done || {}), output: done?.output ?? answered?.output, reason: done?.reason ?? answered?.error }, i, original, promptLang, status).markup;
    });
    grid.insertAdjacentHTML("beforeend", html_.join(""));
  }

  function paintBaseline() {
    const box = $("#baseline", panel);
    if (!run.baseline) return;
    box.hidden = false;
    $("p", box).textContent = run.baseline;
  }

  paintProgress();
  paintCards();
  paintBaseline();
  if (run.phase) say(`phase.${run.phase}`);
  if (run.queued) say("run.queued");

  const off = [
    on("phase", (d) => { run.phase = d.name; say(`phase.${d.name}`); }),
    on("queued", () => say("run.queued")),
    on("plan", () => paintCards()),
    on("baseline", paintBaseline),
    on("progress", paintProgress),
    on("answer", (d) => {
      const card = setCardState(grid, d.id, { status: d.error ? "error" : "answered", output: d.output, reason: d.error });
      card?.classList.add("is-answered");
    }),
    on("test", (test) => setCardState(grid, test.id, test)),
    on("report", () => ctx.go("result")),
    on("failure", (d) => {
      narration.textContent = d.message;
      mascot.setMood("worried");
      mascot.say(t("bubble.oops"));
      ctx.toast(d.message, "bad");
      $("#cancel", panel).textContent = t("plan.back");
      $("#cancel", panel).onclick = () => ctx.go("plan");
    }),
    on("cancelled", () => { ctx.toast(t("err.cancelled")); ctx.go("plan"); }),
  ];

  $("#cancel", panel).onclick = async () => {
    try { await api.cancel(run.id); } catch { /* it may have finished already */ }
  };

  mascot.setMood("thinking");
  mascot.say(t("bubble.thinking"));
  return { dispose() { off.forEach((f) => f()); } };
}
