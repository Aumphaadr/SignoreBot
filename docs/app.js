// Ссылки на скачивание прописаны в HTML и работают без скриптов.
// Этот код — только улучшение: если на GitHub появился релиз новее, он
// подставит его файлы, размеры и заметки. Не ответил GitHub (лимит запросов,
// нет сети) — на странице остаются рабочие ссылки на текущую версию.
(function () {
  const REPO = "Aumphaadr/SignoreBot";
  const FALLBACK = "1.0.2";
  const status = document.getElementById("dl-status");

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
    for (let i = 0; i < Math.max(x.length, y.length); i++) {
      if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) > (y[i] || 0);
    }
    return false;
  };

  fetch("https://api.github.com/repos/" + REPO + "/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error("http " + r.status))))
    .then((rel) => {
      const tag = String(rel.tag_name || rel.name || "").replace(/^v/, "");
      if (!tag || !newer(tag, FALLBACK)) return; // на странице уже актуальные ссылки
      const assets = rel.assets || [];
      let replaced = 0;
      for (const [kind, match] of Object.entries(KINDS)) {
        const a = assets.find((x) => match(x.name.toLowerCase()));
        if (!a) continue;
        replaced++;
        document.querySelectorAll(`[data-dl="${kind}"]`).forEach((el) => { el.href = a.browser_download_url; });
        const box = document.querySelector(`[data-size="${kind}"]`);
        if (box) box.textContent = size(a.size);
      }
      if (!replaced) return;
      const date = rel.published_at ? new Date(rel.published_at).toLocaleDateString("ru-RU") : "";
      status.textContent = `Версия ${tag}${date ? " от " + date : ""}. Файлы отдаёт GitHub, установка не требует ни аккаунта, ни регистрации.`;
      if (rel.body) {
        document.getElementById("dl-notes").hidden = false;
        document.getElementById("dl-notes-text").textContent = rel.body;
      }
    })
    .catch(() => { /* оставляем ссылки из HTML */ });

  // лайтбокс для скриншотов
  const lb = document.getElementById("lightbox"), lbImg = lb.querySelector("img");
  document.querySelectorAll(".shots figure").forEach((f) => f.addEventListener("click", () => {
    const img = f.querySelector("img");
    lbImg.src = img.src; lbImg.alt = img.alt; lb.classList.add("open");
  }));
  lb.addEventListener("click", () => lb.classList.remove("open"));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") lb.classList.remove("open"); });
})();
