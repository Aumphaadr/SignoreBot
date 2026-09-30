#!/usr/bin/env node
// Синхронизация значков SignoreBot с набором Klaarheid Icons.
//
//   npm run icons:sync -- <путь к Klaarheid-Icons> [имя …]
//   node tools/sync-icons.mjs ../../Klaarheid-Icons [имя …]
//
// Берёт из локальной копии набора (git clone; по сети ничего не качает) каждый
// значок, который уже лежит в src/assets/icons/klaarheid/, и новые имена из
// командной строки — и копирует их байт в байт из svg/fill набора. Затем
// пересобирает src/components/Icon/icons.ts и раздел «Значки» в
// THIRD-PARTY-NOTICES.md между метками klaarheid:start и klaarheid:end.
//
// Убрать значок: удалить его файл из src/assets/icons/klaarheid/ и запустить
// синхронизацию. Черновики набора (имена на zz-) не берутся. Знаки Twitch
// (src/assets/icons/twitch/) в набор не входят и здесь не трогаются.

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { checkIconFormat } from "./icon-format.mjs";
import { writeIconsModule } from "./build-icons.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ICON_DIR = path.join(ROOT, "src", "assets", "icons", "klaarheid");
const OWN_DIR = path.join(ROOT, "src", "assets", "icons", "twitch");
const NOTICES_FILE = path.join(ROOT, "THIRD-PARTY-NOTICES.md");
const START = /<!-- klaarheid:start[^>]*-->/u;
const END = "<!-- klaarheid:end -->";
const NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u;

const SET_URL = "https://aumphaadr.github.io/Klaarheid-Icons/";
const REPO_URL = "https://github.com/Aumphaadr/Klaarheid-Icons";

function fail(message) {
  console.error(`sync-icons: ${message}`);
  process.exit(1);
}

/** 1 значок, 2 значка, 5 значков. */
export function plural(n, one, few, many) {
  const tens = n % 100;
  const units = n % 10;
  if (tens >= 11 && tens <= 14) return many;
  if (units === 1) return one;
  if (units >= 2 && units <= 4) return few;
  return many;
}

/** Текст раздела «Значки» между метками — общий с tools/check.mjs. */
export function klaarheidSection(count) {
  return [
    "Значки панели и сайта взяты из набора",
    `[Klaarheid Icons](${SET_URL}) ([репозиторий](${REPO_URL})),`,
    "который разработан автором SignoreBot и опубликован под лицензией **MIT-0**.",
    "Лицензия разрешает любое использование без условий и без упоминания автора.",
    `В SignoreBot ${plural(count, "входит", "входят", "входят")} ${count} ${plural(count, "значок", "значка", "значков")} набора`
      + " в варианте «контур заливкой»; они лежат",
    "в `src/assets/icons/klaarheid/` — это копии файлов набора байт в байт.",
    "",
    "Раздел пересобирается командой `npm run icons:sync`.",
  ].join("\n");
}

function main() {
  const [setDirArg, ...extra] = process.argv.slice(2);
  if (!setDirArg) fail("укажите путь к репозиторию Klaarheid-Icons: npm run icons:sync -- <путь> [имя …]");
  // Набор берётся из локальной копии, по сети ничего не качается: GitHub не
  // хостинг для раздачи файлов — ни сайт набора, ни его репозиторий.
  if (/^[a-z][a-z0-9+.-]*:\/\//iu.test(setDirArg) || /^git@/u.test(setDirArg)) {
    fail(`«${setDirArg}» — адрес, а нужна папка локальной копии набора, например ../Klaarheid-Icons`);
  }
  const setDir = path.resolve(setDirArg);
  const fillDir = path.join(setDir, "svg", "fill");
  if (!fs.existsSync(fillDir)) fail(`в «${setDir}» нет папки svg/fill — это точно Klaarheid-Icons?`);

  // Раздел «Значки» говорит, что на значки действует только MIT-0. Если лицензия
  // набора сменится или набор снова заведёт значки под чужими лицензиями, это
  // станет неправдой — синхронизация останавливается.
  const license = fs.readFileSync(path.join(setDir, "LICENSE"), "utf8");
  if (!/^MIT No Attribution\b/u.test(license)) fail("LICENSE набора — не MIT No Attribution (MIT-0); синхронизация остановлена");
  if (fs.existsSync(path.join(setDir, "src", "third-party.mjs"))) {
    fail("в наборе есть src/third-party.mjs — часть значков снова под другими лицензиями, "
      + "а раздел «Значки» говорит только о MIT-0; синхронизация остановлена");
  }

  fs.mkdirSync(ICON_DIR, { recursive: true });
  const own = new Set(fs.readdirSync(OWN_DIR).filter((f) => f.endsWith(".svg")).map((f) => f.slice(0, -4)));
  const present = fs.readdirSync(ICON_DIR).filter((f) => f.endsWith(".svg")).map((f) => f.slice(0, -4));
  const names = [...new Set([...present, ...extra])].sort();
  for (const name of names) {
    if (!NAME.test(name)) fail(`странное имя значка «${name}»`);
    if (name.startsWith("zz-")) fail(`«${name}» — черновик набора, в проекты он не идёт`);
    if (own.has(name)) fail(`«${name}» — свой знак бота (src/assets/icons/twitch/), из набора его не берут`);
  }

  const sources = new Map();
  for (const name of names) {
    const from = path.join(fillDir, `${name}.svg`);
    if (!fs.existsSync(from)) fail(`в наборе нет значка «${name}» (svg/fill/${name}.svg)`);
    const bytes = fs.readFileSync(from);
    try {
      checkIconFormat(bytes.toString("utf8"));
    } catch (err) {
      fail(`${name}.svg: ${err.message}`);
    }
    sources.set(name, bytes);
  }

  const notices = fs.readFileSync(NOTICES_FILE, "utf8");
  const start = START.exec(notices);
  const endAt = notices.indexOf(END);
  if (!start || endAt < start.index) fail("в THIRD-PARTY-NOTICES.md нет меток klaarheid:start и klaarheid:end");

  // копии байт в байт
  const added = [];
  const changed = [];
  for (const [name, bytes] of sources) {
    const to = path.join(ICON_DIR, `${name}.svg`);
    const old = fs.existsSync(to) ? fs.readFileSync(to) : null;
    if (!old) added.push(name);
    else if (!old.equals(bytes)) changed.push(name);
    fs.writeFileSync(to, bytes);
  }

  writeIconsModule();
  const section = `${start[0]}\n${klaarheidSection(names.length)}\n${END}`;
  fs.writeFileSync(NOTICES_FILE, notices.slice(0, start.index) + section + notices.slice(endAt + END.length));

  console.log(`значков набора: ${names.length} (новых ${added.length}, обновлено ${changed.length})`);
  if (added.length) console.log(`  новые: ${added.join(" ")}`);
  if (changed.length) console.log(`  обновлены: ${changed.join(" ")}`);

  // Предупредить, если бот берёт ещё не опубликованные версии значков.
  try {
    const files = names.map((n) => `svg/fill/${n}.svg`);
    const dirty = execFileSync("git", ["-C", setDir, "status", "--porcelain", "--", ...files], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).split("\n").filter(Boolean).map((l) => l.slice(3));
    if (dirty.length) {
      console.log(`внимание: в рабочей копии набора не закоммичены ${dirty.length} из взятых файлов — ${dirty.join(" ")}`);
    }
  } catch {
    // набор без git — сверять нечего
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
