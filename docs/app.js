// Кнопки «Скачать» берут последний релиз через GitHub API; без релизов — ссылки на страницу релизов.
(function () {
  const REPO = "Aumphaadr/SignoreBot";
  const status = document.getElementById("dl-status");
  const pick = (assets, kind) => assets.find((a) => {
    const n = a.name.toLowerCase();
    if (kind === "deb") return n.endsWith(".deb");
    if (kind === "AppImage") return n.endsWith(".appimage");
    if (kind === "exe") return n.endsWith(".exe") || n.endsWith(".msi") || (n.includes("windows") && n.endsWith(".zip"));
    return false;
  });
  const fmt = (b) => b > 1048576 ? (b / 1048576).toFixed(1) + " МБ" : Math.round(b / 1024) + " КБ";
  fetch("https://api.github.com/repos/" + REPO + "/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => { if (r.status === 404) throw new Error("none"); if (!r.ok) throw new Error("http " + r.status); return r.json(); })
    .then((rel) => {
      const tag = rel.tag_name || rel.name;
      const date = rel.published_at ? new Date(rel.published_at).toLocaleDateString("ru-RU") : "";
      status.textContent = "Последний релиз: " + tag + (date ? " от " + date : "") + ".";
      document.querySelectorAll("[data-asset]").forEach((el) => {
        const a = pick(rel.assets || [], el.dataset.asset);
        el.textContent = a ? a.name + " · " + fmt(a.size) : "в этом релизе файла нет";
      });
      document.querySelectorAll("[data-link]").forEach((el) => {
        const a = pick(rel.assets || [], el.dataset.link);
        if (a) { el.href = a.browser_download_url; el.textContent = "⬇ Скачать " + tag; el.classList.add("primary"); }
      });
      if (rel.body) { document.getElementById("dl-notes").hidden = false; document.getElementById("dl-notes-text").textContent = rel.body; }
    })
    .catch((e) => {
      status.textContent = e.message === "none" ? "Релизов пока нет — следите за страницей релизов на GitHub." : "Не удалось проверить релизы (" + e.message + "). Откройте страницу релизов вручную.";
      document.querySelectorAll("[data-asset]").forEach((el) => { el.textContent = "—"; });
    });

  // лайтбокс для скриншотов
  const lb = document.getElementById("lightbox"), lbImg = lb.querySelector("img");
  document.querySelectorAll(".shots figure").forEach((f) => f.addEventListener("click", () => { lbImg.src = f.querySelector("img").src; lbImg.alt = f.querySelector("img").alt; lb.classList.add("open"); }));
  lb.addEventListener("click", () => lb.classList.remove("open"));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") lb.classList.remove("open"); });
})();
