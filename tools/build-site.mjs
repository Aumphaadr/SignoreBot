// Собирает сайт из site/ в docs/ (GitHub Pages).
//
// Каждый урок — настоящая страница docs/<slug>/index.html, поэтому прямые
// ссылки открываются без JavaScript и без трюков с 404. Внутри сайта
// docs/app.js перехватывает переходы и подменяет содержимое через History API,
// не перезагружая страницу. Ссылки относительные ({{root}}), так что сайт
// работает и по адресу /SignoreBot/ на GitHub Pages, и с любого статического
// сервера.
//
// Источники: site/pages.json (порядок и заголовки), site/template.html
// (каркас), site/pages/<slug>.html (содержимое <main>), site/style.css,
// site/app.js. Картинки, шрифты и логотип лежат сразу в docs/.
//
// Версия — только из package.json: ссылки на файлы релиза, размеры
// (site/release.json), дата и «что нового» (раздел CHANGELOG этой версии)
// подставляются на карточки скачивания, а docs/version.json с теми же данными
// читает приложение при проверке обновлений. Раздела в CHANGELOG нет — сборка
// останавливается: сайт не должен обещать версию, о которой нечего сказать.
// Запуск: npm run site
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "site");
const OUT = join(ROOT, "docs");

const pages = JSON.parse(readFileSync(join(SRC, "pages.json"), "utf8"));

// ─── версия, релиз, «что нового» ────────────────────────────────────────────
const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
const VERSION = pkg.version;
const REPO = /github\.com\/([^/]+\/[^/.]+)/.exec(pkg.repository?.url ?? "")?.[1];
if (!REPO) throw new Error("package.json: repository.url должен указывать на GitHub — от него строятся ссылки на файлы релиза");
const SITE = (pkg.homepage ?? "").replace(/\/?$/, "/");
if (!SITE.startsWith("http")) throw new Error("package.json: нет homepage — адрес сайта нужен для version.json");

const changelogLines = readFileSync(join(ROOT, "CHANGELOG.md"), "utf8").split("\n");
const sectionAt = changelogLines.findIndex((l) => l.startsWith(`## ${VERSION} — `));
const DATE = sectionAt >= 0 ? /— (\d{4}-\d{2}-\d{2})\s*$/.exec(changelogLines[sectionAt])?.[1] : undefined;
if (!DATE) throw new Error(`CHANGELOG.md: нет раздела «## ${VERSION} — ГГГГ-ММ-ДД» — заполните его перед сборкой сайта`);
const sectionEnd = changelogLines.findIndex((l, i) => i > sectionAt && l.startsWith("## "));
const NOTES = changelogLines.slice(sectionAt + 1, sectionEnd < 0 ? changelogLines.length : sectionEnd).join("\n").trim();
if (!NOTES) throw new Error(`CHANGELOG.md: раздел ${VERSION} пуст`);
const dateRu = (iso) => iso.split("-").reverse().join(".");

const FILES = {
  setup: `SignoreBot_${VERSION}-windows-x64-setup.exe`,
  portable: `SignoreBot_${VERSION}-windows-x64-portable.exe`,
  zip: `SignoreBot_${VERSION}-windows-x64-portable.zip`,
  deb: `SignoreBot_${VERSION}-linux-amd64.deb`,
  appimage: `SignoreBot_${VERSION}-linux-amd64.AppImage`,
};
const fileUrl = (kind) => `https://github.com/${REPO}/releases/download/v${VERSION}/${FILES[kind]}`;
const RELEASE_URL = `https://github.com/${REPO}/releases/tag/v${VERSION}`;
const SIZES = JSON.parse(readFileSync(join(SRC, "release.json"), "utf8")).sizes;
const mb = (bytes) => (bytes ? `${(bytes / 1048576).toFixed(1)} МБ` : "");

/** Пункты CHANGELOG (markdown с **жирным** и `кодом`) → HTML-список. */
function notesHtml(md) {
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const inline = (s) => esc(s).replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/`([^`]+)`/g, "<code>$1</code>");
  const items = [];
  for (const line of md.split("\n")) {
    if (/^- /.test(line)) items.push(line.slice(2));
    else if (line.trim() && items.length) items[items.length - 1] += ` ${line.trim()}`;
  }
  return `<ul>\n${items.map((i) => `<li>${inline(i)}</li>`).join("\n")}\n</ul>`;
}

const versionJson = {
  version: VERSION,
  date: DATE,
  page: `${SITE}#download`,
  release: RELEASE_URL,
  files: Object.fromEntries(Object.keys(FILES).map((k) => [k, fileUrl(k)])),
  sizes: Object.fromEntries(Object.keys(FILES).map((k) => [k, SIZES[k] ?? 0])),
  notes: NOTES,
};
const template = readFileSync(join(SRC, "template.html"), "utf8");
const lessons = pages.filter((p) => p.lesson);

// Иконки — тот же набор, что в приложении (src/components/Icon/icons.ts,
// его генерирует tools/build-icons.mjs). В страницах: {{icon:download}}.
const ICONS = JSON.parse(
  readFileSync(join(ROOT, "src/components/Icon/icons.ts"), "utf8")
    .replace(/^[\s\S]*?export const ICONS = /, "")
    .replace(/\} as const;[\s\S]*$/, "}")
    .replace(/,(\s*\})/g, "$1"),
);
const icon = (name) => {
  const i = ICONS[name];
  if (!i) throw new Error(`нет иконки «${name}»`);
  return `<svg class="ico" viewBox="${i[0]}" fill="currentColor" aria-hidden="true"${i[2] ? " " + i[2] : ""}>${i[1]}</svg>`;
};

const render = (tpl, vars) =>
  tpl.replace(/\{\{icon:([a-z0-9-]+)\}\}/g, (m, n) => icon(n)).replace(/\{\{(\w+)\}\}/g, (m, k) => (k in vars ? vars[k] : m));
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const href = (root, p) => (p.slug ? `${root}${p.slug}/` : root);

for (const p of pages) {
  const root = p.slug ? "../" : "./";
  const content = readFileSync(join(SRC, "pages", `${p.slug || "index"}.html`), "utf8");
  const i = lessons.indexOf(p);
  const prev = i > 0 ? lessons[i - 1] : null;
  const next = i >= 0 && i < lessons.length - 1 ? lessons[i + 1] : null;

  // Список уроков: в шапке главной и в боковой колонке уроков.
  const lessonList = lessons
    .map((l) => `<li${l === p ? ' class="active"' : ""}><a href="${href(root, l)}"><span class="n">${l.lesson}</span> ${esc(l.nav)}</a></li>`)
    .join("\n");
  const prevnext = p.lesson
    ? `<nav class="prevnext" aria-label="Соседние уроки">
  ${prev ? `<a class="prev" href="${href(root, prev)}"><small>Назад</small>${esc(prev.title)}</a>` : "<span></span>"}
  ${next ? `<a class="next" href="${href(root, next)}"><small>Дальше</small>${esc(next.title)}</a>` : `<a class="next" href="${root}#download"><small>Дальше</small>Скачать и попробовать</a>`}
</nav>`
    : "";

  // Карта уроков для главной ({{lessonsGrid}} в pages/index.html).
  const lessonsGrid = lessons
    .map((l) => `<a href="${href(root, l)}"><span class="h"><span class="n">${l.lesson}</span><b>${esc(l.title)}</b></span><span class="d">${esc(l.description)}</span></a>`)
    .join("\n");

  const vars = {
    root,
    slug: p.slug,
    version: VERSION,
    date: dateRu(DATE),
    notesHtml: notesHtml(NOTES),
    releaseUrl: RELEASE_URL,
    releasesUrl: `https://github.com/${REPO}/releases`,
    dlSetup: fileUrl("setup"),
    dlPortable: fileUrl("portable"),
    dlZip: fileUrl("zip"),
    dlDeb: fileUrl("deb"),
    dlAppimage: fileUrl("appimage"),
    fileDeb: FILES.deb,
    fileAppimage: FILES.appimage,
    sizeSetup: mb(SIZES.setup),
    sizePortable: mb(SIZES.portable),
    sizeDeb: mb(SIZES.deb),
    sizeAppimage: mb(SIZES.appimage),
    lessonsGrid,
    bodyClass: p.lesson ? "lesson" : "home",
    // Главная во всю ширину: у секций своя .wrap внутри, иначе фон-градиент
    // героя обрывался бы по краю колонки, а не уходил за край экрана.
    pageClass: p.lesson ? "page wrap" : "page",
    title: esc(p.lesson ? `${p.title} · SignoreBot` : p.title),
    description: esc(p.description),
    lessonList,
    lessonAside: p.lesson
      ? `<aside class="lessons-aside"><div class="aside-title">Уроки</div><ol class="lesson-list">\n${lessonList}\n</ol></aside>`
      : "",
    lessonHeader: p.lesson ? `<p class="lesson-kicker">Урок ${p.lesson} из ${lessons.length}</p>` : "",
    prevnext,
  };
  // Содержимое рендерится отдельно: подстановка — один проход, и плейсхолдеры
  // внутри уже подставленного текста не раскрываются.
  vars.content = render(content, vars);
  const html = render(template, vars);
  const left = html.match(/\{\{[^}]+\}\}/g);
  if (left) throw new Error(`${p.slug || "index"}: не подставлено ${left.join(", ")}`);

  const dir = p.slug ? join(OUT, p.slug) : OUT;
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "index.html"), html);
}

for (const f of ["style.css", "app.js"]) copyFileSync(join(SRC, f), join(OUT, f));
writeFileSync(join(OUT, "version.json"), `${JSON.stringify(versionJson, null, 2)}\n`);
console.log(`Страниц: ${pages.length} (уроков: ${lessons.length}), версия ${VERSION} от ${DATE} → docs/`);
