// Генерирует из src-tauri/fonts/manifest.json:
//  - src-tauri/src/overlay/fonts_gen.rs  — байты шрифтов (include_bytes) + @font-face для страницы оверлея;
//  - src/styles/overlay-fonts.css        — те же @font-face для предпросмотра в панели (Vite упакует файлы);
//  - src/api/fonts.generated.ts          — список семейств для выпадающего списка.
// Запуск: npm run fonts
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const man = JSON.parse(readFileSync(join(ROOT, "src-tauri/fonts/manifest.json"), "utf8"));

const face = (fam, f, url) => `@font-face{font-family:"${fam}";src:url("${url}") format("truetype");font-weight:${f.weight};font-style:${f.style};font-display:swap}`;
const overlayCss = man.flatMap((m) => m.faces.map((f) => face(m.family, f, `/fonts/${f.file}`))).join("\n");
const panelCss = man.flatMap((m) => m.faces.map((f) => face(m.family, f, `../../src-tauri/fonts/${f.file}`))).join("\n");
const files = man.flatMap((m) => m.faces.map((f) => f.file));

writeFileSync(join(ROOT, "src-tauri/src/overlay/fonts_gen.rs"), [
  "// СГЕНЕРИРОВАНО tools/build-fonts.mjs — не редактировать вручную.",
  "/// Файлы шрифтов, встроенные в бинарник; отдаются оверлею по /fonts/<имя>.",
  "pub static FONTS: &[(&str, &[u8])] = &[",
  ...files.map((f) => `    (${JSON.stringify(f)}, include_bytes!(${JSON.stringify("../../fonts/" + f)})),`),
  "];",
  "/// @font-face для страницы оверлея (пути относительно сервера оверлеев).",
  `pub const FONT_FACE_CSS: &str = ${JSON.stringify(overlayCss)};`,
  "",
].join("\n"));
writeFileSync(join(ROOT, "src/styles/overlay-fonts.css"), "/* СГЕНЕРИРОВАНО tools/build-fonts.mjs — те же шрифты, что отдаёт оверлею сервер */\n" + panelCss + "\n");
writeFileSync(join(ROOT, "src/api/fonts.generated.ts"), [
  "// СГЕНЕРИРОВАНО tools/build-fonts.mjs — не редактировать вручную.",
  "/** Шрифты, встроенные в приложение: одинаковы в предпросмотре и на оверлее в OBS. */",
  "export const FONT_FAMILIES: { label: string; value: string }[] = [",
  ...man.map((m) => `  { label: ${JSON.stringify(m.label)}, value: ${JSON.stringify(`"${m.family}", sans-serif`)} },`),
  "];",
  "",
].join("\n"));
console.log(`Шрифтов: ${files.length} файлов, ${man.length} семейств`);
