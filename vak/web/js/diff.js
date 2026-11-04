// A word-level diff, used to show what a variation changed in the original question.
import { esc, raw } from "./dom.js";

function words(text) {
  return text.split(/\s+/).filter(Boolean);
}

// Marks the words of `variant` that are not in `original` (longest common subsequence).
export function highlightChanges(original, variant) {
  const a = words(original);
  const b = words(variant);
  if (!b.length) return raw("");
  const rows = a.length + 1;
  const cols = b.length + 1;
  const table = Array.from({ length: rows }, () => new Uint16Array(cols));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const kept = new Set();
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) { kept.add(j); i++; j++; }
    else if (table[i + 1][j] >= table[i][j + 1]) i++;
    else j++;
  }
  const out = [];
  let run = [];
  const flush = () => { if (run.length) { out.push(`<mark>${esc(run.join(" "))}</mark>`); run = []; } };
  b.forEach((word, idx) => {
    if (kept.has(idx)) { flush(); out.push(esc(word)); } else run.push(word);
  });
  flush();
  return raw(out.join(" "));
}
