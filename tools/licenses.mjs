#!/usr/bin/env node
// Библиотеки, которые уходят к пользователю, и их лицензии.
//
//   npm run licenses          — пересобрать раздел «Библиотеки» в THIRD-PARTY-NOTICES.md
//                               (между метками deps:start и deps:end) и third-party/rust-crates.md
//   node tools/licenses.mjs --check — только сверить (так его зовёт npm run check)
//
// Что считается «уходит к пользователю»:
//  - npm: пакеты из package-lock.json без пометки dev — то, что Vite кладёт в панель;
//  - Rust: крейты, которые линкуются в бинарник SignoreBot под Linux и Windows —
//    `cargo tree -e normal,no-proc-macro`: обычные зависимости (не dev и не build)
//    без процедурных макросов, с настоящим разбором features. cargo metadata для
//    этого не годится: он не отделяет features макросов от программы и приписывает
//    бинарнику лишние крейты (так было: 423 вместо 363).
// Каждая лицензия сверяется с белым списком ALLOWED; MPL-2.0 — только у крейтов
// из MPL_OK (слабый копилефт по файлам: используем без изменений). Новая лицензия
// или новый крейт под MPL — провал, пока человек не посмотрит и не впишет его сюда.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const NOTICES = join(ROOT, "THIRD-PARTY-NOTICES.md");
const CRATES_FILE = "third-party/rust-crates.md";
const START = "<!-- deps:start -->";
const END = "<!-- deps:end -->";
const TARGETS = [
  ["x86_64-unknown-linux-gnu", "Linux"],
  ["x86_64-pc-windows-msvc", "Windows"],
];

/** Разрешительные лицензии (SPDX): свободное использование, в том числе в коммерческих продуктах. */
export const ALLOWED = new Set([
  "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD",
  "Unlicense", "BSL-1.0", "CC0-1.0", "Unicode-3.0", "Unicode-DFS-2016", "CDLA-Permissive-2.0",
  "Apache-2.0 WITH LLVM-exception",
]);
/** Крейты под MPL-2.0, которые можно брать: откуда приходят (MPL — слабый копилефт по файлам, используем без изменений). */
export const MPL_OK = new Map([["option-ext", "приходит с Tauri через dirs — пути к системным папкам"]]);

// ─── SPDX-выражения ─────────────────────────────────────────────────────────

function tokenize(expr) {
  const src = expr.replace(/\s*\/\s*/g, " OR "); // старая запись «MIT/Apache-2.0»
  return src.match(/\(|\)|[A-Za-z0-9.+-]+/g) ?? [];
}

/** Разбор в дерево: { or: [...] } | { and: [...] } | { id } (WITH склеен в id). */
export function parseLicense(expr) {
  const t = tokenize(expr);
  let i = 0;
  const atom = () => {
    if (t[i] === "(") { i++; const e = orExpr(); if (t[i++] !== ")") throw new Error(`скобки в «${expr}»`); return e; }
    let id = t[i++];
    if (!id) throw new Error(`обрыв в «${expr}»`);
    if (t[i] === "WITH") { id = `${id} WITH ${t[i + 1]}`; i += 2; }
    return { id };
  };
  const andExpr = () => { const xs = [atom()]; while (t[i] === "AND") { i++; xs.push(atom()); } return xs.length > 1 ? { and: xs } : xs[0]; };
  const orExpr = () => { const xs = [andExpr()]; while (t[i] === "OR") { i++; xs.push(andExpr()); } return xs.length > 1 ? { or: xs } : xs[0]; };
  const tree = orExpr();
  if (i !== t.length) throw new Error(`лишнее в «${expr}»`);
  return tree;
}

/** Каноническая запись: варианты OR и части AND по алфавиту — чтобы «MIT OR Apache-2.0» и «Apache-2.0/MIT» совпали. */
export function canonical(node, top = true) {
  if (node.id) return node.id;
  const op = node.or ? "OR" : "AND";
  const parts = (node.or ?? node.and).map((n) => canonical(n, false)).sort();
  const s = parts.join(` ${op} `);
  return top ? s : `(${s})`;
}

/** Можно ли выбрать лицензию из белого списка (для OR — хотя бы одну ветку). */
export function allowed(node, crate) {
  if (node.id) return ALLOWED.has(node.id) || (node.id === "MPL-2.0" && MPL_OK.has(crate));
  if (node.or) return node.or.some((n) => allowed(n, crate));
  return node.and.every((n) => allowed(n, crate));
}

// ─── сбор ───────────────────────────────────────────────────────────────────

export function collectNpm() {
  const lock = JSON.parse(readFileSync(join(ROOT, "package-lock.json"), "utf8"));
  const out = [];
  for (const [key, p] of Object.entries(lock.packages)) {
    if (!key || p.dev || p.devOptional) continue;
    out.push({ name: key.split("node_modules/").pop(), version: p.version, license: p.license ?? "" });
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

/** null, если cargo недоступен. */
export function collectCrates() {
  const crates = new Map();
  const direct = new Set();
  for (const [triple, os] of TARGETS) {
    const tree = (extra) => execFileSync("cargo", ["tree", "--locked", "-e", "normal,no-proc-macro", "--target", triple,
      "--prefix", "none", "--format", "{p}|{l}", ...extra, "--manifest-path", join(ROOT, "src-tauri", "Cargo.toml")],
    { encoding: "utf8", maxBuffer: 64 << 20, stdio: ["ignore", "pipe", "pipe"] });
    let all;
    let top;
    try {
      all = tree([]);
      top = tree(["--depth", "1"]);
    } catch (err) {
      if (err.code === "ENOENT") return null;
      throw new Error(`cargo tree (${triple}): ${String(err.stderr || err.message).trim().split("\n").pop()}`);
    }
    // строка: «имя vВерсия|лицензия», у своих крейтов после версии — путь в скобках
    const parse = (text) => text.split("\n").map((line) => {
      const bar = line.lastIndexOf("|");
      const m = /^(\S+) v(\S+)( \(.*\))?/.exec(line.slice(0, bar));
      return m && !(m[3] && !m[3].includes("*")) ? { name: m[1], version: m[2], license: line.slice(bar + 1).trim() } : null;
    }).filter(Boolean);
    for (const c of parse(all)) {
      const key = `${c.name}@${c.version}`;
      const e = crates.get(key) ?? { ...c, os: [] };
      if (!e.os.includes(os)) e.os.push(os);
      crates.set(key, e);
    }
    for (const c of parse(top)) direct.add(`${c.name}@${c.version}`);
  }
  const list = [...crates.values()].sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version, undefined, { numeric: true }));
  return { list, direct };
}

/** Проблемы с лицензиями: ["крейт 1.0 — GPL-3.0", …]. */
export function problems(items, kind) {
  const out = [];
  for (const x of items) {
    let ok = false;
    try { ok = x.license && allowed(parseLicense(x.license), x.name); } catch { ok = false; }
    if (!ok) out.push(`${kind} ${x.name} ${x.version} — «${x.license || "лицензия не указана"}»`);
  }
  return out;
}

// ─── вывод ──────────────────────────────────────────────────────────────────

/** 1 крейт, 2 крейта, 5 крейтов. */
function plural(n, one, few, many) {
  const tens = n % 100;
  const units = n % 10;
  if (tens >= 11 && tens <= 14) return many;
  if (units === 1) return one;
  if (units >= 2 && units <= 4) return few;
  return many;
}

const lic = (s) => { try { return canonical(parseLicense(s)); } catch { return s; } };
const WHY = {
  "@tauri-apps/api": "мост панели к ядру", "@tauri-apps/plugin-clipboard-manager": "буфер обмена",
  "@tauri-apps/plugin-dialog": "окна выбора файлов", "@tauri-apps/plugin-opener": "ссылки в браузере",
  react: "интерфейс панели", "react-dom": "интерфейс панели", scheduler: "приходит с ReactDOM",
};

export function renderSection(npm, crates) {
  const L = [START, ""];
  L.push("### Панель (npm)", "");
  L.push("В панель при сборке попадают только эти пакеты; средства разработки (TypeScript, Vite,",
    "svgo и другие из `devDependencies`) к пользователю не уходят.", "");
  L.push("| Пакет | Версия | Зачем | Лицензия |", "|---|---|---|---|");
  for (const p of npm) L.push(`| ${p.name} | ${p.version} | ${WHY[p.name] ?? ""} | ${lic(p.license)} |`);
  L.push("", "### Ядро (Rust)", "");
  const direct = crates.list.filter((c) => crates.direct.has(`${c.name}@${c.version}`));
  const n = crates.list.length;
  L.push(`В бинарник SignoreBot под Linux и Windows ${plural(n, "линкуется", "линкуются", "линкуются")} ${n} ${plural(n, "крейт", "крейта", "крейтов")}. Прямые зависимости`,
    "проекта (остальные приходят с ними):", "");
  L.push("| Крейт | Версия | Лицензия |", "|---|---|---|");
  for (const c of direct) L.push(`| ${c.name} | ${c.version} | ${lic(c.license)} |`);
  const by = new Map();
  for (const c of crates.list) { const k = lic(c.license); by.set(k, (by.get(k) ?? 0) + 1); }
  L.push("", "Все крейты по лицензиям (выражения SPDX; «A OR B» — можно выбрать любую):", "");
  L.push("| Лицензия | Крейтов |", "|---|---|");
  for (const [k, n] of [...by].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))) L.push(`| ${k} | ${n} |`);
  const mpl = crates.list.filter((c) => /MPL-2\.0/.test(c.license));
  if (mpl.length) {
    L.push("", `Под MPL-2.0 — ${mpl.map((c) => `${c.name} ${c.version} (${MPL_OK.get(c.name) ?? "?"})`).join(", ")}.`,
      "MPL — слабый копилефт по файлам: такие крейты используются без изменений, их исходники открыты на crates.io.");
  }
  L.push("", `Полный список с версиями — [${CRATES_FILE}](${CRATES_FILE}).`, "", END);
  return L.join("\n");
}

export function renderCratesFile(crates) {
  const L = [
    "# Крейты Rust в бинарнике SignoreBot",
    "",
    "Сгенерировано `npm run licenses` (tools/licenses.mjs) из `cargo tree -e normal,no-proc-macro` —",
    "руками не править. Крейты, которые линкуются в программу под Linux (x86_64-unknown-linux-gnu)",
    "и Windows (x86_64-pc-windows-msvc); то, что нужно только при сборке (процедурные макросы и",
    "build-зависимости), сюда не входит. Лицензии — как их объявили авторы крейтов (SPDX).",
    "",
    `Всего: ${crates.list.length}.`,
    "",
    "| Крейт | Версия | Лицензия | Где |",
    "|---|---|---|---|",
    ...crates.list.map((c) => `| ${c.name} | ${c.version} | ${lic(c.license)} | ${c.os.length === TARGETS.length ? "везде" : c.os.join(", ")} |`),
    "",
  ];
  return L.join("\n");
}

function replaceSection(text, section) {
  const a = text.indexOf(START);
  const b = text.indexOf(END);
  if (a < 0 || b < a) throw new Error("в THIRD-PARTY-NOTICES.md нет меток deps:start и deps:end");
  return text.slice(0, a) + section + text.slice(b + END.length);
}

/** Сверка для npm run check: список проблем (пустой — всё хорошо). */
export function checkLicenses() {
  const errors = [];
  const npm = collectNpm();
  errors.push(...problems(npm, "npm"));
  const crates = collectCrates();
  if (!crates) return { errors, skipped: "cargo не найден — лицензии крейтов и список в notices не сверены" };
  errors.push(...problems(crates.list, "крейт"));
  const notices = readFileSync(NOTICES, "utf8");
  if (replaceSection(notices, renderSection(npm, crates)) !== notices) errors.push("раздел «Библиотеки» в THIRD-PARTY-NOTICES.md устарел — npm run licenses");
  const file = join(ROOT, CRATES_FILE);
  if (!existsSync(file) || readFileSync(file, "utf8") !== renderCratesFile(crates)) errors.push(`${CRATES_FILE} устарел — npm run licenses`);
  return { errors, crates: crates.list.length, npm: npm.length };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.includes("--check")) {
    const r = checkLicenses();
    if (r.skipped) console.log(`лицензии: ${r.skipped}`);
    if (r.errors.length) { console.error(r.errors.join("\n")); process.exit(1); }
    if (!r.skipped) console.log(`лицензии: пакетов npm — ${r.npm}, крейтов Rust — ${r.crates}; все из белого списка, списки свежие`);
  } else {
    const npm = collectNpm();
    const crates = collectCrates();
    if (!crates) { console.error("licenses: нужен cargo (Rust) — список крейтов берётся из cargo metadata"); process.exit(1); }
    const bad = [...problems(npm, "npm"), ...problems(crates.list, "крейт")];
    if (bad.length) { console.error(`лицензии вне белого списка — посмотрите и решите:\n${bad.join("\n")}`); process.exit(1); }
    writeFileSync(NOTICES, replaceSection(readFileSync(NOTICES, "utf8"), renderSection(npm, crates)));
    mkdirSync(join(ROOT, "third-party"), { recursive: true });
    writeFileSync(join(ROOT, CRATES_FILE), renderCratesFile(crates));
    console.log(`лицензии: пакетов npm — ${npm.length}, крейтов Rust — ${crates.list.length} → THIRD-PARTY-NOTICES.md, ${CRATES_FILE}`);
  }
}
