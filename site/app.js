// Сайт SignoreBot: маршрутизация поверх настоящих страниц, лайтбокс, свежий релиз.
//
// Каждый урок лежит отдельной страницей (docs/<slug>/index.html), поэтому
// сайт работает и без этого скрипта. Скрипт делает переходы мгновенными:
// перехватывает клик по внутренней ссылке, загружает страницу fetch'ем,
// подменяет шапку, содержимое и подвал, а адрес в строке браузера меняет
// через History API (pushState). Кнопка «Назад» работает через popstate.
(function () {
  "use strict";

  // ------------------------------------------------------------ маршрутизация
  const cache = new Map();
  const isPage = (u) => /\/$|\.html$/.test(u.pathname);

  async function fetchPage(url) {
    if (cache.has(url)) return cache.get(url);
    const r = await fetch(url, { credentials: "same-origin" });
    if (!r.ok) throw new Error("http " + r.status);
    const html = await r.text();
    cache.set(url, html);
    return html;
  }

  function swap(doc) {
    for (const sel of ["header.top", "#page", "footer"]) {
      const cur = document.querySelector(sel);
      const nxt = doc.querySelector(sel);
      if (cur && nxt) cur.replaceWith(nxt);
    }
    document.title = doc.title;
    const d = document.querySelector('meta[name="description"]');
    const nd = doc.querySelector('meta[name="description"]');
    if (d && nd) d.setAttribute("content", nd.getAttribute("content"));
    document.body.className = doc.body.className;
    document.body.dataset.root = doc.body.dataset.root || "./";
    enhance();
  }

  async function go(href, push) {
    const url = new URL(href, location.href);
    let html;
    try {
      html = await fetchPage(url.origin + url.pathname);
    } catch {
      location.href = url.href; // страница не загрузилась — обычный переход
      return;
    }
    const doc = new DOMParser().parseFromString(html, "text/html");
    if (!doc.getElementById("page")) {
      location.href = url.href;
      return;
    }
    // Сначала адрес, потом DOM: относительные ссылки и картинки новой
    // страницы должны разрешаться уже от нового адреса.
    if (push) history.pushState({ y: 0 }, "", url.href);
    swap(doc);
    const target = url.hash && document.getElementById(url.hash.slice(1));
    if (target) target.scrollIntoView();
    else window.scrollTo(0, push ? 0 : (history.state && history.state.y) || 0);
    const main = document.getElementById("main");
    if (main) { main.setAttribute("tabindex", "-1"); main.focus({ preventScroll: true }); }
  }

  document.addEventListener("click", (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = e.target.closest("a[href]");
    if (!a || a.target || a.hasAttribute("download")) return;
    const url = new URL(a.href, location.href);
    if (url.origin !== location.origin || !isPage(url)) return;
    // Тот же путь, другой якорь — пусть браузер прокрутит сам.
    if (url.pathname === location.pathname && url.hash) return;
    e.preventDefault();
    history.replaceState({ y: window.scrollY }, "");
    go(url.href, true);
  });

  document.addEventListener("mouseover", (e) => {
    const a = e.target.closest("a[href]");
    if (!a) return;
    const url = new URL(a.href, location.href);
    if (url.origin === location.origin && isPage(url) && !cache.has(url.origin + url.pathname)) fetchPage(url.origin + url.pathname).catch(() => {});
  });

  window.addEventListener("popstate", () => go(location.href, false));
  if ("scrollRestoration" in history) history.scrollRestoration = "manual";

  // ------------------------------------------------------------ лайтбокс
  const box = document.getElementById("lightbox");
  document.addEventListener("click", (e) => {
    const img = e.target.closest("figure.shot img, .shots img");
    if (img && box) {
      box.querySelector("img").src = img.src;
      box.querySelector("img").alt = img.alt;
      box.classList.add("open");
      return;
    }
    if (box && e.target.closest("#lightbox")) box.classList.remove("open");
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && box) box.classList.remove("open"); });

  // ------------------------------------------------------------ свежий релиз
  // Ссылки на скачивание прописаны в HTML и работают без скриптов. Этот код —
  // только улучшение: если на GitHub появился релиз новее, он подставит его
  // файлы, размеры и заметки. Не ответил GitHub (лимит запросов, нет сети) —
  // на странице остаются рабочие ссылки на текущую версию.
  const REPO = "Aumphaadr/SignoreBot";
  const KINDS = {
    deb: (n) => n.endsWith(".deb"),
    appimage: (n) => n.endsWith(".appimage"),
    setup: (n) => n.endsWith(".exe") && n.includes("setup"),
    portable: (n) => (n.endsWith(".exe") || n.endsWith(".zip")) && n.includes("portable"),
  };
  const size = (b) => (b > 1048576 ? (b / 1048576).toFixed(1) + " МБ" : Math.round(b / 1024) + " КБ");
  const num = (v) => String(v).replace(/^v/, "").split(/[.-]/).map((x) => parseInt(x, 10) || 0);
  const newer = (a, b) => {
    const [x, y] = [num(a), num(b)];
    for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) > (y[i] || 0);
    return false;
  };
  let release = null; // Promise с ответом GitHub, один на всё время жизни страницы

  function enhanceDownloads() {
    const status = document.getElementById("dl-status");
    if (!status) return;
    const fallback = status.dataset.version || "0";
    if (!release) {
      release = fetch("https://api.github.com/repos/" + REPO + "/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
        .then((r) => (r.ok ? r.json() : Promise.reject(new Error("http " + r.status))));
    }
    release.then((rel) => {
      const tag = String(rel.tag_name || rel.name || "").replace(/^v/, "");
      if (!tag || !newer(tag, fallback)) return;
      const assets = rel.assets || [];
      let replaced = 0;
      for (const [kind, match] of Object.entries(KINDS)) {
        const a = assets.find((x) => match(x.name.toLowerCase()));
        if (!a) continue;
        replaced++;
        document.querySelectorAll(`[data-dl="${kind}"]`).forEach((el) => { el.href = a.browser_download_url; });
        const sz = document.querySelector(`[data-size="${kind}"]`);
        if (sz) sz.textContent = size(a.size);
      }
      if (!replaced) return;
      const date = rel.published_at ? new Date(rel.published_at).toLocaleDateString("ru-RU") : "";
      status.textContent = `Версия ${tag}${date ? " от " + date : ""}. Файлы отдаёт GitHub, установка не требует ни аккаунта, ни регистрации.`;
      const notes = document.getElementById("dl-notes");
      if (rel.body && notes) { notes.hidden = false; document.getElementById("dl-notes-text").textContent = rel.body; }
    }).catch(() => { /* оставляем ссылки из HTML */ });
  }

  function enhance() {
    enhanceDownloads();
  }
  enhance();
})();
