#!/usr/bin/env node
// Стражи SignoreBot: npm run check.
//
//  1. Внешние адреса. Панель, страницы оверлея и сайт ничего не грузят с других
//     сайтов — ни с CDN, ни с GitHub, в том числе с сайта и из репозитория набора
//     Klaarheid: GitHub не хостинг для раздачи файлов. Можно: ссылку для перехода
//     (<a href>, openUrl), комментарий, пространство имён XML, адрес этого компьютера
//     (127.0.0.1, localhost — сервер оверлеев, OBS). Сайт узнаёт версию из своей
//     сборки (version.json), в API GitHub не ходит.
//  2. Значки. icons.ts собран из файлов; значки набора — в формате набора, раздел
//     «Значки» в THIRD-PARTY-NOTICES.md сходится с папкой; лишних значков нет.
//  3. Шрифты. У каждого файла шрифта рядом лежит лицензия, и notices её называют.
//  4. Лицензии библиотек. Пакеты npm из панели и крейты Rust из бинарника — только
//     из белого списка (tools/licenses.mjs), раздел «Библиотеки» свежий.
// Каждый страж проверен на обратном: самопроверки ниже должны его ронять.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { extname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { renderIconsModule, OUT as ICONS_TS } from "./build-icons.mjs";
import { checkIconFormat } from "./icon-format.mjs";
import { klaarheidSection } from "./sync-icons.mjs";
import { allowed, checkLicenses, parseLicense } from "./licenses.mjs";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const read = (rel) => readFileSync(join(ROOT, rel), "utf8");
let failures = 0;
const ok = (msg) => console.log(`✓ ${msg}`);
const bad = (msg, list = []) => {
  failures++;
  console.log(`✗ ${msg}${list.length ? `\n    ${list.join("\n    ")}` : ""}`);
};

function walk(dir, exts, out = []) {
  if (!existsSync(join(ROOT, dir))) return out;
  for (const e of readdirSync(join(ROOT, dir), { withFileTypes: true })) {
    const rel = `${dir}/${e.name}`;
    if (e.isDirectory()) walk(rel, exts, out);
    else if (exts.has(extname(e.name))) out.push(rel);
  }
  return out;
}

// ─── 1. внешние адреса ──────────────────────────────────────────────────────

const SERVED_DIRS = ["src", "public", "src-tauri/overlay", "site", "docs"];
const SERVED_EXT = new Set([".html", ".css", ".js", ".mjs", ".ts", ".tsx", ".svg", ".json"]);
const NAMESPACE = /^http:\/\/(?:www\.w3\.org\/(?:2000\/svg|1999\/xlink|1999\/xhtml|XML\/1998\/namespace)|www\.inkscape\.org\/namespaces\/inkscape|sodipodi\.sourceforge\.net\/DTD\/sodipodi-0\.dtd|creativecommons\.org\/ns#|purl\.org\/dc\/elements\/1\.1\/|www\.w3\.org\/1999\/02\/22-rdf-syntax-ns#)$/u;
const LOOPBACK = /^(?:[a-z]+:)?\/\/(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])(?:[:/?#]|$)/iu;
// Полный адрес или адрес без протокола: «//cdn.example.com/…».
const ADDRESS = /(?:\b[a-z][a-z0-9+.-]*:)?\/\/[a-z0-9-]+(?:\.[a-z0-9-]+)+(?::\d+)?[^\s"'`)<>]*|(?:\b[a-z][a-z0-9+.-]*:)?\/\/(?:localhost|\[::1\])(?::\d+)?[^\s"'`)<>]*/giu;
const COMMENT_LINE = /^\s*(?:\/\/|\/\*|\*|<!--|\{\/\*)/u;
const TRAILING_COMMENT = /(?:^|[\s,;{}()[\]])\/\/\s/u;

/** Адрес стоит в href ссылки: от последнего «<» до адреса — открытый тег <a с href. */
function inAnchorHref(text, index) {
  const open = text.lastIndexOf("<", index);
  if (open < 0) return false;
  return /^<a\s[^<>]*\bhref\s*=\s*\{?\s*["'`]?$/iu.test(text.slice(open, index));
}
/** Адрес открывается в браузере пользователя, а не грузится страницей. */
const inNavigation = (text, index) => /(?:openUrl|window\.open)\(\s*["'`]$/u.test(text.slice(Math.max(0, index - 40), index));

export function externalAddresses(text) {
  const found = [];
  for (const m of text.matchAll(ADDRESS)) {
    const addr = m[0];
    if (NAMESPACE.test(addr) || LOOPBACK.test(addr) || inAnchorHref(text, m.index) || inNavigation(text, m.index)) continue;
    const lineStart = text.lastIndexOf("\n", m.index - 1) + 1;
    const lineEnd = text.indexOf("\n", m.index);
    const line = text.slice(lineStart, lineEnd < 0 ? text.length : lineEnd);
    if (COMMENT_LINE.test(line) || TRAILING_COMMENT.test(text.slice(lineStart, m.index))) continue;
    found.push({ line: text.slice(0, m.index).split("\n").length, address: addr });
  }
  return found;
}

function checkLocalOnly() {
  // docs/version.json — данные для приложения: адреса файлов релиза, которые скачивает
  // человек (как <a href>), страница их не грузит. Проверяется отдельно: есть и
  // называет версию из package.json.
  const files = ["index.html", ...SERVED_DIRS.flatMap((d) => walk(d, SERVED_EXT))].filter((f) => f !== "docs/version.json").sort();
  const hits = [];
  for (const rel of files) for (const h of externalAddresses(read(rel))) hits.push(`${rel}:${h.line} — ${h.address}`);
  if (hits.length) bad("внешние адреса — всё, что нужно странице, кладётся в репозиторий, значки Klaarheid копируются через npm run icons:sync", hits);
  else ok(`внешних загрузок нет (файлов: ${files.length})`);
  // Проверка не пустая: в обходе панель, оверлей, сайт и его сборка.
  const must = ["index.html", "src/main.tsx", "src/components/Icon/icons.ts", "src-tauri/overlay/overlay.html", "site/app.js", "docs/index.html", "public/favicon.svg"];
  const missing = must.filter((m) => !files.includes(m));
  if (missing.length) bad("страж внешних адресов не видит файлов", missing);
  const pkgVersion = JSON.parse(read("package.json")).version;
  const vj = existsSync(join(ROOT, "docs/version.json")) ? JSON.parse(read("docs/version.json")) : null;
  if (!vj) bad("нет docs/version.json — npm run site");
  else if (vj.version !== pkgVersion) bad(`docs/version.json называет ${vj.version}, а package.json — ${pkgVersion}: npm run site`);
  else ok(`docs/version.json: версия ${vj.version} от ${vj.date}, файлов ${Object.keys(vj.files ?? {}).length}`);

  // самопроверка на обратном
  const caught = (text) => externalAddresses(text).map((h) => h.address);
  const set = "https://aumphaadr.github.io/Klaarheid-Icons/svg/fill/eye.svg";
  const raw = "https://raw.githubusercontent.com/Aumphaadr/Klaarheid-Icons/main/svg/fill/eye.svg";
  const cases = [
    [caught(`<img src="${set}" alt="">`), [set], "картинка с сайта набора"],
    [caught(`const svg = await (await fetch('${raw}')).text();`), [raw], "файл из репозитория набора"],
    [caught('<script src="https://cdn.jsdelivr.net/gh/Aumphaadr/Klaarheid-Icons/svg/fill/x.svg"></script>'), ["https://cdn.jsdelivr.net/gh/Aumphaadr/Klaarheid-Icons/svg/fill/x.svg"], "jsDelivr gh/"],
    [caught(".x { background: url(//cdn.example.com/a.png) }"), ["//cdn.example.com/a.png"], "адрес без протокола в CSS"],
    [caught('<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Jost">'), ["https://fonts.googleapis.com/css2?family=Jost"], "шрифт с чужого сервера"],
    [caught("fetch('https://api.github.com/repos/Aumphaadr/SignoreBot/releases/latest')"), ["https://api.github.com/repos/Aumphaadr/SignoreBot/releases/latest"], "API GitHub — тоже загрузка"],
    [caught('<a href="https://github.com/Aumphaadr/SignoreBot">исходники</a>'), [], "ссылка для перехода"],
    [caught('<a className="x" href={"https://www.twitch.tv/x"}>канал</a>'), [], "ссылка в JSX"],
    [caught('void openUrl("https://dev.twitch.tv/console");'), [], "открыть в браузере"],
    [caught("// https://example.com/doc"), [], "строка комментария"],
    [caught("{/* https://example.com/doc */}"), [], "комментарий JSX"],
    [caught('<code>http://127.0.0.1:3001/overlay/a?key=…</code> ws://localhost:4455'), [], "адрес этого компьютера"],
    [caught('<svg xmlns="http://www.w3.org/2000/svg" xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd">'), [], "пространства имён"],
  ];
  const wrong = cases.filter(([got, want]) => JSON.stringify(got) !== JSON.stringify(want)).map(([got, want, what]) => `${what}: ждали ${JSON.stringify(want)}, получили ${JSON.stringify(got)}`);
  if (wrong.length) bad("самопроверка стража внешних адресов", wrong);
}

// ─── 2. значки ──────────────────────────────────────────────────────────────

function checkIcons() {
  const KL = "src/assets/icons/klaarheid";
  const OWN = "src/assets/icons/twitch";
  const names = (d) => readdirSync(join(ROOT, d)).filter((f) => f.endsWith(".svg")).map((f) => f.slice(0, -4));
  const kl = names(KL);
  const own = names(OWN);
  const formatErr = [];
  for (const [d, list] of [[KL, kl], [OWN, own]]) {
    for (const n of list) {
      try { checkIconFormat(read(`${d}/${n}.svg`)); } catch (e) { formatErr.push(`${d}/${n}.svg: ${e.message}`); }
    }
  }
  if (formatErr.length) bad("значки не в формате набора (сетка 24, одни <path> цвета currentColor)", formatErr);
  let text;
  try { text = renderIconsModule().text; } catch (e) { bad(`значки не собираются: ${e.message}`); return; }
  if (read(ICONS_TS) !== text) bad(`${ICONS_TS} не совпадает с файлами значков — npm run icons`);
  else ok(`${ICONS_TS} собран из файлов (набор: ${kl.length}, свои: ${own.length})`);

  const notices = read("THIRD-PARTY-NOTICES.md");
  const m = /<!-- klaarheid:start[^>]*-->\n([\s\S]*?)\n<!-- klaarheid:end -->/u.exec(notices);
  if (!m) bad("в THIRD-PARTY-NOTICES.md нет раздела между klaarheid:start и klaarheid:end");
  else if (m[1] !== klaarheidSection(kl.length)) bad("раздел «Значки» в THIRD-PARTY-NOTICES.md расходится с папкой — npm run icons:sync");
  else ok(`раздел «Значки» сходится с папкой (${kl.length})`);

  // Каждый значок где-то нужен: имя встречается строкой в панели или {{icon:…}} на сайте.
  const code = walk("src", new Set([".ts", ".tsx"])).filter((f) => f !== ICONS_TS).map(read).join("\n");
  const site = walk("site", new Set([".html"])).map(read).join("\n");
  const unused = [...kl, ...own].filter((n) => !code.includes(`"${n}"`) && !code.includes(`'${n}'`) && !site.includes(`{{icon:${n}}}`));
  if (unused.length) bad("значки лежат, но нигде не используются — удалите файл и запустите npm run icons", unused);
  else ok("лишних значков нет");
}

// ─── 3. шрифты ──────────────────────────────────────────────────────────────

function checkFonts() {
  const dirs = ["src/assets/fonts", "src-tauri/fonts", "docs/fonts"];
  const notices = read("THIRD-PARTY-NOTICES.md");
  const errors = [];
  let n = 0;
  for (const d of dirs) {
    const files = readdirSync(join(ROOT, d));
    for (const f of files.filter((x) => /\.(ttf|otf|woff2?)$/u.test(x))) {
      n++;
      const stem = f.split(/[-.]/u)[0];
      const license = files.find((x) => x === `${stem}-OFL.txt` || x === `${stem}-LICENSE.txt`);
      if (!license) { errors.push(`${d}/${f}: рядом нет ${stem}-OFL.txt`); continue; }
      if (!notices.includes(`${d}/${license}`)) errors.push(`${d}/${license}: THIRD-PARTY-NOTICES.md о нём молчит`);
    }
  }
  const manifest = JSON.parse(read("src-tauri/fonts/manifest.json"));
  for (const fam of manifest) {
    if (!existsSync(join(ROOT, "src-tauri/fonts", fam.license))) errors.push(`manifest.json: у «${fam.family}» нет файла ${fam.license}`);
  }
  if (errors.length) bad("шрифты без лицензии рядом", errors);
  else ok(`у всех ${n} файлов шрифтов лицензия рядом и строка в notices`);
}

// ─── 4. лицензии библиотек ──────────────────────────────────────────────────

function checkDeps() {
  const self = [
    ["MIT", "x", true], ["MIT/Apache-2.0", "x", true], ["Apache-2.0 WITH LLVM-exception", "x", true],
    ["MIT OR GPL-3.0", "x", true], ["GPL-3.0", "x", false], ["MIT AND GPL-3.0", "x", false],
    ["LGPL-2.1-or-later", "x", false], ["MPL-2.0", "option-ext", true], ["MPL-2.0", "cssparser", false],
    ["(MIT OR Apache-2.0) AND Unicode-3.0", "x", true], ["AGPL-3.0-only OR SSPL-1.0", "x", false],
  ];
  const wrong = self.filter(([e, c, want]) => allowed(parseLicense(e), c) !== want).map(([e, c, want]) => `${e} (${c}): ждали ${want}`);
  if (wrong.length) bad("самопроверка белого списка лицензий", wrong);
  const r = checkLicenses();
  if (r.skipped) console.log(`- ${r.skipped}`);
  if (r.errors.length) bad("лицензии библиотек", r.errors);
  else if (!r.skipped) ok(`лицензии: пакетов npm — ${r.npm}, крейтов Rust — ${r.crates}; все из белого списка, раздел «Библиотеки» свежий`);
}

checkLocalOnly();
checkIcons();
checkFonts();
checkDeps();
console.log(failures ? `\nПровалено проверок: ${failures}` : "\nВсе проверки пройдены");
process.exit(failures ? 1 : 0);
