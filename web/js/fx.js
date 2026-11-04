import { reducedMotion } from "./dom.js";

let ctx;
/** Small sounds made with the Web Audio API; silent when the browser won't allow audio yet. */
export function sfx(kind) {
  try {
    ctx ||= new (window.AudioContext || window.webkitAudioContext)();
    const notes = { ok: [523, 659, 784], wrong: [220, 175], tap: [440], done: [523, 659, 784, 1047] }[kind] || [440];
    notes.forEach((f, i) => {
      const o = ctx.createOscillator(), g = ctx.createGain();
      o.type = kind === "wrong" ? "triangle" : "sine"; o.frequency.value = f;
      g.gain.setValueAtTime(0.0001, ctx.currentTime + i * 0.09);
      g.gain.exponentialRampToValueAtTime(0.07, ctx.currentTime + i * 0.09 + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + i * 0.09 + 0.22);
      o.connect(g).connect(ctx.destination); o.start(ctx.currentTime + i * 0.09); o.stop(ctx.currentTime + i * 0.09 + 0.25);
    });
  } catch { /* no audio: fine */ }
}

export function confetti(count = 46) {
  if (reducedMotion()) return;
  const colors = ["#f0d7ff", "#ffa946", "#cef5ca", "#034f46", "#1a1a1a"];
  const layer = document.createElement("div");
  layer.style.cssText = "position:fixed;inset:0;pointer-events:none;z-index:95;overflow:hidden";
  for (let i = 0; i < count; i++) {
    const p = document.createElement("i");
    const x = 50 + (Math.random() - .5) * 30;
    p.style.cssText = `position:absolute;left:${x}%;top:55%;width:.55rem;height:.9rem;border-radius:2px;background:${colors[i % colors.length]};`;
    layer.append(p);
    p.animate([{ transform: "translate(0,0) rotate(0)", opacity: 1 }, { transform: `translate(${(Math.random() - .5) * 60}vw, ${(Math.random() - .8) * 70}vh) rotate(${Math.random() * 720}deg)`, opacity: 0 }], { duration: 1100 + Math.random() * 700, easing: "cubic-bezier(.2,.7,.3,1)", fill: "forwards" });
  }
  document.body.append(layer);
  setTimeout(() => layer.remove(), 2100);
}

export function toast(message, ms = 4200) {
  const t = document.createElement("div");
  t.className = "toast"; t.setAttribute("role", "status"); t.textContent = message;
  document.body.append(t);
  setTimeout(() => t.remove(), ms);
}

/** Fades sections in as they scroll into view. */
export function reveal(root = document) {
  const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); } }), { threshold: 0.12 });
  root.querySelectorAll(".reveal").forEach((n) => io.observe(n));
}
