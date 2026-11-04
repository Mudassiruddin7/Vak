// Small flourishes: floating Indic letters, the cursor-following glow, count-up numbers, confetti.
import { reducedMotion } from "./dom.js";

const GLYPHS = ["అ", "ఆ", "క", "వ", "మ", "ర", "తె", "లు", "అ", "ఆ", "क", "व", "म", "र", "हि", "वा", "₹", "అ", "क", "వా"];
const TINTS = ["var(--leaf-500)", "var(--leaf-400)", "var(--haldi)", "var(--saffron)", "var(--leaf-600)"];

// a small seeded generator so the letters sit in the same places on every visit
function rng(seed) {
  let s = seed;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
}

export function glyphField(host) {
  const rand = rng(20240611);
  host.replaceChildren();
  GLYPHS.forEach((g, i) => {
    const span = document.createElement("span");
    span.className = "glyph";
    span.textContent = g;
    const size = 34 + rand() * 90;
    span.style.cssText = [
      `left:${(rand() * 96).toFixed(1)}%`, `top:${(rand() * 94).toFixed(1)}%`, `font-size:${size.toFixed(0)}px`,
      `color:${TINTS[i % TINTS.length]}`, `--dx:${(rand() * 60 - 30).toFixed(0)}px`, `--dy:${(rand() * 60 - 30).toFixed(0)}px`,
      `--r0:${(rand() * 30 - 15).toFixed(0)}deg`, `--r1:${(rand() * 30 - 15).toFixed(0)}deg`,
      `--dur:${(18 + rand() * 18).toFixed(1)}s`, `--delay:${(-rand() * 20).toFixed(1)}s`,
    ].join(";");
    host.append(span);
  });
}

export function followPointerGlow() {
  if (reducedMotion()) return;
  let ticking = false;
  window.addEventListener("pointermove", (e) => {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(() => {
      const root = document.documentElement.style;
      root.setProperty("--mx", `${((e.clientX / window.innerWidth) * 100).toFixed(1)}%`);
      root.setProperty("--my", `${((e.clientY / window.innerHeight) * 100).toFixed(1)}%`);
      ticking = false;
    });
  }, { passive: true });
}

export function countUp(node, to, { ms = 1500, from = 0, decimals = 0 } = {}) {
  if (reducedMotion()) { node.textContent = to.toFixed(decimals); return; }
  const start = performance.now();
  const ease = (x) => (x >= 1 ? 1 : 1 - Math.pow(2, -10 * x));
  const frame = (now) => {
    const p = Math.min(1, (now - start) / ms);
    node.textContent = (from + (to - from) * ease(p)).toFixed(decimals);
    if (p < 1) requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

export function confetti(count = 54) {
  if (reducedMotion()) return;
  const layer = document.createElement("div");
  layer.className = "confetti";
  layer.setAttribute("aria-hidden", "true");
  const colors = ["#6aa82e", "#a6d26f", "#ffc531", "#ff8a3d", "#f2699b", "#2fb5a6", "#4651d8"];
  for (let i = 0; i < count; i++) {
    const piece = document.createElement("i");
    piece.style.cssText = [
      `left:${(Math.random() * 100).toFixed(1)}%`, `background:${colors[i % colors.length]}`,
      `--x:${(Math.random() * 160 - 80).toFixed(0)}px`, `--rot:${(Math.random() * 900 - 300).toFixed(0)}deg`,
      `--t:${(1.8 + Math.random() * 1.4).toFixed(2)}s`, `--d:${(Math.random() * 0.5).toFixed(2)}s`,
      Math.random() < 0.35 ? "border-radius:50%;width:10px;height:10px" : "",
    ].join(";");
    layer.append(piece);
  }
  document.body.append(layer);
  setTimeout(() => layer.remove(), 4200);
}
