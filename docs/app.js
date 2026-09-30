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


  // Карточки скачивания: определяем ОС посетителя и приглушаем чужие.
  function detectOs() {
    const p = ((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "").toLowerCase();
    const ua = navigator.userAgent.toLowerCase();
    if (p.includes("win") || ua.includes("windows")) return "windows";
    if (p.includes("linux") || ua.includes("linux") || ua.includes("x11")) return "linux";
    if (p.includes("mac") || ua.includes("mac os")) return "mac";
    return "";
  }
  function enhanceOsCards() {
    const os = detectOs();
    const cards = document.querySelectorAll(".dl .card[data-os]");
    if (!cards.length) return;
    cards.forEach((c) => c.classList.toggle("other-os", !!os && os !== "mac" && c.dataset.os !== os));
    const note = document.getElementById("dl-os-note");
    if (note) note.textContent = os === "windows" ? "Похоже, у вас Windows — подходящая карточка первая." : os === "linux" ? "Похоже, у вас Linux — подходящие карточки выделены." : "";
  }

  // Иконка вкладки задана относительным путём ({{root}}logo.svg), а <head>
  // при переходах не меняется: браузер перезапрашивал её от адреса урока и
  // получал 404. Один раз делаем адрес абсолютным.
  const icon = document.querySelector('link[rel="icon"]');
  if (icon) icon.setAttribute("href", icon.href);

  function enhance() {
    enhanceOsCards();
  }
  enhance();
})();
