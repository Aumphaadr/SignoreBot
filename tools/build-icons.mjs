// Собирает значки в один src/components/Icon/icons.ts.
// Запуск: npm run icons (или сам, из npm run icons:sync); проверка: node tools/build-icons.mjs --check
//
// Источники:
//  - src/assets/icons/klaarheid/ — значки набора Klaarheid Icons (MIT-0), копии файлов
//    svg/fill набора байт в байт; кладёт и обновляет их только npm run icons:sync;
//  - src/assets/icons/twitch/ — знаки Bits и баллов канала, нарисованные для SignoreBot
//    (в набор они не входят: это обозначения функций Twitch).
//
// Что делает:
//  - прогоняет svgo (чистит разметку). Контуры svgo НЕ пересчитывает (`convertPathData`
//    выключен): значки построены из отрезков и дуг, а пересчёт склеивает кривые
//    окружностей в приблизительные дуги и сдвигает края;
//  - убирает жёсткий цвет и свойство color, чтобы значок красился через currentColor;
//  - viewBox оставляет как есть: у набора поле уже внутри кадра 24×24.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { optimize } from "svgo";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
export const ICON_DIRS = ["src/assets/icons/klaarheid", "src/assets/icons/twitch"];
export const OUT = "src/components/Icon/icons.ts";

const svgoConfig = {
  multipass: true,
  plugins: [
    { name: "preset-default", params: { overrides: { convertPathData: false } } },
    { name: "convertStyleToAttrs" },
    { name: "removeDimensions" },
  ],
};

/** Текст icons.ts по текущим файлам значков: { text, count, warnings }. */
export function renderIconsModule() {
  const icons = [];
  const seen = new Map();
  let warnings = 0;
  for (const dir of ICON_DIRS) {
    for (const file of readdirSync(join(ROOT, dir)).filter((f) => f.endsWith(".svg")).sort()) {
      const name = file.replace(/\.svg$/, "");
      if (seen.has(name)) throw new Error(`значок «${name}» лежит и в ${seen.get(name)}, и в ${dir}`);
      seen.set(name, dir);
      const path = join(ROOT, dir, file);
      const { data } = optimize(readFileSync(path, "utf8"), { ...svgoConfig, path });
      const open = data.slice(0, data.indexOf(">") + 1);
      const body = data.slice(data.indexOf(">") + 1, data.lastIndexOf("</svg>")).trim()
        .replace(/\s(?<!-)color="[^"]*"/g, "")
        .replace(/(?<!-)color\s*:\s*[^;"]+;?/g, "")
        .replace(/style="\s*"/g, "")
        .replace(/\s+>/g, ">");
      const vb = /viewBox="([^"]+)"/.exec(open);
      if (!vb) throw new Error(`${dir}/${file}: нет viewBox`);
      // атрибуты корня, кроме служебных: у штриховых значков это fill="none" и stroke-*
      const attrs = [...open.matchAll(/([a-zA-Z:-]+)="([^"]*)"/g)]
        .filter(([, k]) => !["xmlns", "xmlns:xlink", "viewBox", "width", "height", "version", "id", "xml:space"].includes(k))
        .map(([, k, v]) => `${k}="${v}"`)
        .join(" ");
      const hard = body.match(/(?:fill|stroke|color|stop-color)\s*[:=]\s*"?(#[0-9a-fA-F]{3,8}|black|rgb\()/g);
      if (hard) {
        console.warn(`  ! ${name}: жёсткий цвет внутри (${[...new Set(hard)].join(", ")}) — значок не примет цвет темы`);
        warnings++;
      }
      icons.push({ name, viewBox: vb[1], body, attrs });
    }
  }
  icons.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  const text = [
    "// СГЕНЕРИРОВАНО tools/build-icons.mjs — не редактировать вручную.",
    "// Источники: src/assets/icons/klaarheid/*.svg (набор Klaarheid Icons, npm run icons:sync)",
    "// и src/assets/icons/twitch/*.svg; пересобрать: npm run icons",
    "",
    "/** [viewBox, содержимое, атрибуты корня] */",
    "export const ICONS = {",
    ...icons.map((i) => `  ${JSON.stringify(i.name)}: [${JSON.stringify(i.viewBox)}, ${JSON.stringify(i.body)}, ${JSON.stringify(i.attrs)}],`),
    "} as const;",
    "",
    "export type IconName = keyof typeof ICONS;",
    "",
  ].join("\n");
  return { text, count: icons.length, warnings };
}

/** Записать icons.ts; возвращает число значков. */
export function writeIconsModule() {
  const { text, count, warnings } = renderIconsModule();
  writeFileSync(join(ROOT, OUT), text);
  const kb = (Buffer.byteLength(text) / 1024).toFixed(0);
  console.log(`Значков: ${count}; ${OUT} — ${kb} КБ` + (warnings ? `; предупреждений: ${warnings}` : ""));
  return count;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.includes("--check")) {
    const { text } = renderIconsModule();
    if (readFileSync(join(ROOT, OUT), "utf8") !== text) {
      console.error(`${OUT} не совпадает с файлами значков — запустите npm run icons`);
      process.exit(1);
    }
    console.log(`${OUT} совпадает с файлами значков`);
  } else {
    writeIconsModule();
  }
}
