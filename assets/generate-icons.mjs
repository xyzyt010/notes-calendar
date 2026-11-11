// Fetches Lucide SVGs via gh api and generates apps/shared-ui/icons.slint.
// Lucide license: ISC.  https://lucide.dev/
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(fileURLToPath(import.meta.url)) + "/..";
const ICON_DIR = join(ROOT, "assets", "icons");
const OUT = join(ROOT, "apps", "shared-ui", "icons.slint");
mkdirSync(ICON_DIR, { recursive: true });
mkdirSync(join(ROOT, "apps", "shared-ui"), { recursive: true });

// Icons we need (Lucide names).
const ICONS = [
  "ghost", "search", "plus", "pin", "pin-off", "archive", "archive-arrow-up", "trash", "settings",
  "layout-grid", "list", "file-text", "folder", "folder-open", "folder-plus", "tag", "star", "check",
  "x", "chevron-left", "chevron-right", "chevron-down", "chevron-up", "chevrons-up-down", "menu",
  "panel-left", "panel-left-close",
  "calendar", "calendar-days", "clock", "bell", "bell-ring", "alarm-clock", "map-pin", "repeat",
  "pencil", "link", "link-2",
  "external-link", "download", "upload", "moon", "sun", "info",
  "timer", "circle-check", "circle", "ellipsis", "share", "copy", "undo-2", "redo-2",
  "list-ordered", "square", "square-check", "heading-1", "wifi-off", "text-align-start",
];

const num = (v) => parseFloat(v);
const n = (v) => Math.round(v * 1000) / 1000;

// path element → path data
function circleToPath(cx, cy, r) {
  return `M ${n(cx + r)} ${n(cy)} A ${n(r)} ${n(r)} 0 1 0 ${n(cx - r)} ${n(cy)} A ${n(r)} ${n(r)} 0 1 0 ${n(cx + r)} ${n(cy)} Z`;
}
function ellipseToPath(cx, cy, rx, ry) {
  return `M ${n(cx + rx)} ${n(cy)} A ${n(rx)} ${n(ry)} 0 1 0 ${n(cx - rx)} ${n(cy)} A ${n(rx)} ${n(ry)} 0 1 0 ${n(cx + rx)} ${n(cy)} Z`;
}
function rectToPath(x, y, w, h, rx, ry) {
  // clamp radii
  rx = Math.min(num(rx || 0), num(w) / 2);
  ry = Math.min(num(ry ?? rx), num(h) / 2);
  if (rx === 0 && ry === 0) return `M ${n(num(x))} ${n(num(y))} H ${n(num(x) + num(w))} V ${n(num(y) + num(h))} H ${n(num(x))} Z`;
  const [X, Y, W, H] = [num(x), num(y), num(w), num(h)];
  return `M ${n(X + rx)} ${n(Y)} H ${n(X + W - rx)} A ${n(rx)} ${n(ry)} 0 0 1 ${n(X + W)} ${n(Y + ry)} V ${n(Y + H - ry)} A ${n(rx)} ${n(ry)} 0 0 1 ${n(X + W - rx)} ${n(Y + H)} H ${n(X + rx)} A ${n(rx)} ${n(ry)} 0 0 1 ${n(X)} ${n(Y + H - ry)} V ${n(Y + ry)} A ${n(rx)} ${n(ry)} 0 0 1 ${n(X + rx)} ${n(Y)} Z`;
}
function lineToPath(x1, y1, x2, y2) {
  return `M ${n(num(x1))} ${n(num(y1))} L ${n(num(x2))} ${n(num(y2))}`;
}
function polylineToPath(points, close) {
  const pts = points.trim().split(/[\s,]+/).map(Number);
  let d = "";
  for (let i = 0; i < pts.length; i += 2) {
    d += (i === 0 ? "M " : " L ") + n(pts[i]) + " " + n(pts[i + 1]);
  }
  if (close) d += " Z";
  return d;
}

function elementsToPath(svg) {
  const inner = svg.replace(/^[\s\S]*?<svg[^>]*>/, "").replace(/<\/svg>[\s\S]*$/, "");
  const parts = [];
  const tagRe = /<(path|circle|ellipse|rect|line|polyline|polygon)([^>]*?)\/>/g;
  let m;
  const attr = (attrs, name) => {
    const am = attrs.match(new RegExp(name + '="([^"]*)"'));
    return am ? am[1] : undefined;
  };
  while ((m = tagRe.exec(inner)) !== null) {
    const [, tag, attrs] = m;
    if (tag === "path") parts.push(attr(attrs, "d"));
    else if (tag === "circle") parts.push(circleToPath(num(attr(attrs, "cx")), num(attr(attrs, "cy")), num(attr(attrs, "r"))));
    else if (tag === "ellipse") parts.push(ellipseToPath(num(attr(attrs, "cx")), num(attr(attrs, "cy")), num(attr(attrs, "rx")), num(attr(attrs, "ry"))));
    else if (tag === "rect") {
      const x = num(attr(attrs, "x") ?? "0"), y = num(attr(attrs, "y") ?? "0");
      const w = num(attr(attrs, "width")), h = num(attr(attrs, "height"));
      parts.push(rectToPath(x, y, w, h, attr(attrs, "rx"), attr(attrs, "ry")));
    } else if (tag === "line") parts.push(lineToPath(attr(attrs, "x1"), attr(attrs, "y1"), attr(attrs, "x2"), attr(attrs, "y2")));
    else if (tag === "polyline") parts.push(polylineToPath(attr(attrs, "points"), false));
    else if (tag === "polygon") parts.push(polylineToPath(attr(attrs, "points"), true));
  }
  return parts.join(" ");
}

function pascal(name) {
  return name.split("-").map((s) => s.charAt(0).toUpperCase() + s.slice(1)).join("");
}

const banner = `// Generated from Lucide icons (ISC License, https://lucide.dev) by assets/generate-icons.mjs — do not edit by hand.
// Each icon is a stroke-based vector matching Lucide's 24x24, stroke-width 2, round caps/joins.

`;

let out = banner;
let failures = [];
for (const name of ICONS) {
  const file = join(ICON_DIR, `${name}.svg`);
  if (!existsSync(file)) {
    try {
      const svg = execFileSync("gh", ["api", `repos/lucide-icons/lucide/contents/icons/${name}.svg`, "-H", "Accept: application/vnd.github.raw"], { encoding: "utf8", maxBuffer: 1 << 22 });
      writeFileSync(file, svg);
    } catch (e) {
      failures.push(name);
      console.error("FAILED", name, String(e).slice(0, 120));
      continue;
    }
  }
  const svg = readFileSync(file, "utf8");
  const d = elementsToPath(svg);
  if (!d.trim()) { failures.push(name); console.error("EMPTY", name); continue; }
  out += `export component Icon${pascal(name)} inherits Path {
    in property <color> stroke-color: #e8eaed;
    stroke: stroke-color;
    stroke-width: 2px;
    stroke-line-cap: round;
    stroke-line-join: round;
    viewbox-width: 24;
    viewbox-height: 24;
    fit: contain;
    horizontal-stretch: 0;
    vertical-stretch: 0;
    commands: "${d.replace(/"/g, '\\"')}";
    preferred-width: 24px;
    preferred-height: 24px;
}

`;
}
writeFileSync(OUT, out);
console.log("wrote", OUT);
if (failures.length) process.exitCode = 1;
