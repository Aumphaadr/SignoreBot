// Собирает src/assets/icons/*.svg в один src/components/Icon/icons.ts.
// Запуск: npm run icons
//
// Что делает:
//  - прогоняет svgo (чистит метаданные Inkscape, режет точность координат);
//  - убирает чёрную заливку и свойство color, чтобы иконка красилась через
//    currentColor (color="#000" внутри перебивал бы цвет темы);
//  - расширяет viewBox на PAD с каждой стороны: исходники нарисованы впритык
//    к краю кадра, а рядом с текстом нужно поле — иначе иконки выглядят
//    крупнее и теснее, чем всё вокруг.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { optimize } from "svgo";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "src/assets/icons");
const OUT = join(ROOT, "src/components/Icon/icons.ts");
const PAD = 0.045; // доля стороны кадра с каждой стороны (≈92% полезной площади, как у прежнего набора)

const svgoConfig = {
  multipass: true,
  plugins: [
    { name: "preset-default", params: { overrides: { removeViewBox: false, convertPathData: { floatPrecision: 2 } } } },
    { name: "convertStyleToAttrs" },
    { name: "removeDimensions" },
  ],
};

const round = (n) => Number(n.toFixed(2));
const files = readdirSync(SRC).filter((f) => f.endsWith(".svg")).sort();
const icons = [];
let warnings = 0;

for (const file of files) {
  const name = file.replace(/\.svg$/, "");
  const { data } = optimize(readFileSync(join(SRC, file), "utf8"), { ...svgoConfig, path: join(SRC, file) });
  const open = data.slice(0, data.indexOf(">") + 1);
  const body = data.slice(data.indexOf(">") + 1, data.lastIndexOf("</svg>")).trim();

  // Inkscape пишет color:#000000 в style; svgo превращает это в color="#000",
  // и тогда currentColor у детей резолвится в чёрный. Убираем свойство целиком.
  let cleanBody = body
    .replace(/\s(?<!-)color="[^"]*"/g, "")
    .replace(/(?<!-)color\s*:\s*[^;"]+;?/g, "")
    .replace(/style="\s*"/g, "")
    .replace(/\s+>/g, ">");

  const vb = /viewBox="([^"]+)"/.exec(open);
  if (!vb) throw new Error(`${file}: нет viewBox`);
  const [x, y, w, h] = vb[1].split(/[\s,]+/).map(Number);
  const viewBox = [round(x - w * PAD), round(y - h * PAD), round(w * (1 + 2 * PAD)), round(h * (1 + 2 * PAD))].join(" ");

  // атрибуты корня, кроме служебных: у штриховых иконок это fill="none" и stroke-*
  const attrs = [...open.matchAll(/([a-zA-Z:-]+)="([^"]*)"/g)]
    .filter(([, k]) => !["xmlns", "xmlns:xlink", "viewBox", "width", "height", "version", "id", "xml:space"].includes(k))
    .map(([, k, v]) => `${k}="${v}"`)
    .join(" ");

  const hard = cleanBody.match(/(?:fill|stroke|color|stop-color)\s*[:=]\s*"?(#[0-9a-fA-F]{3,8}|black|rgb\()/g);
  if (hard) {
    console.warn(`  ! ${name}: жёсткий цвет внутри (${[...new Set(hard)].join(", ")}) — иконка не примет цвет темы`);
    warnings++;
  }
  icons.push({ name, viewBox, body: cleanBody, attrs });
}

const lines = [
  "// СГЕНЕРИРОВАНО tools/build-icons.mjs — не редактировать вручную.",
  "// Источники: src/assets/icons/*.svg; пересобрать: npm run icons",
  "",
  "/** [viewBox, содержимое, атрибуты корня] */",
  "export const ICONS = {",
  ...icons.map((i) => `  ${JSON.stringify(i.name)}: [${JSON.stringify(i.viewBox)}, ${JSON.stringify(i.body)}, ${JSON.stringify(i.attrs)}],`),
  "} as const;",
  "",
  "export type IconName = keyof typeof ICONS;",
  "",
];
writeFileSync(OUT, lines.join("\n"));
const kb = (Buffer.byteLength(lines.join("\n")) / 1024).toFixed(0);
console.log(`Иконок: ${icons.length}; ${OUT.replace(ROOT + "/", "")} — ${kb} КБ` + (warnings ? `; предупреждений: ${warnings}` : ""));
