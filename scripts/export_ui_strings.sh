#!/usr/bin/env bash
# Prints the English interface strings that get machine-translated for the other languages, as JSON.
#   scripts/export_ui_strings.sh > /tmp/ui-en.json
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
sed 's#"./i18n.js"#"./i18n.mjs"#' "$here/web/js/strings.js" > "$tmp/strings.mjs"
cp "$here/web/js/i18n.js" "$tmp/i18n.mjs"
cat > "$tmp/run.mjs" <<'JS'
import { TABLES } from "./i18n.mjs";
import "./strings.mjs";
const PREFIXES = ["nav.", "announce.", "h.", "how.", "tr.", "s.", "a.", "effort.", "diff.", "st.", "au.", "r.", "l.", "grade.", "state.", "stat.", "phase.", "grp.", "cat.", "err.", "explain.", "flags.", "words.", "untested.", "tried.", "detail."];
const out = {};
for (const [k, v] of Object.entries(TABLES.en)) if (PREFIXES.some((p) => k.startsWith(p)) && typeof v === "string") out[k] = v;
process.stdout.write(JSON.stringify(out, null, 1));
JS
node "$tmp/run.mjs"
rm -rf "$tmp"
