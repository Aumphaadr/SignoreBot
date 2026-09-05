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
// Запуск: npm run site
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "site");
const OUT = join(ROOT, "docs");

const pages = JSON.parse(readFileSync(join(SRC, "pages.json"), "utf8"));
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
  tpl.replace(/\{\{icon:([a-z-]+)\}\}/g, (m, n) => icon(n)).replace(/\{\{(\w+)\}\}/g, (m, k) => (k in vars ? vars[k] : m));
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
console.log(`Страниц: ${pages.length} (уроков: ${lessons.length}) → docs/`);
