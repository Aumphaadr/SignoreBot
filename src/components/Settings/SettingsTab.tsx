// Сеть, ключ оверлеев, Twitch Client ID, поведение оверлея, экспорт/импорт.

import Icon from "../Icon";
import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText, type DataDirInfo, type UpdateInfo } from "../../api";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";

export default function SettingsTab() {
  const { config, setSection, status, reloadConfig } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const net = config.network;
  const ov = config.overlaySettings;
  const [port, setPort] = useState(String(net.httpPort));
  const [clientId, setClientId] = useState(config.twitch.clientId);
  const upd = config.updates;
  const [repoUrl, setRepoUrl] = useState(upd.repoUrl);
  const [update, setUpdate] = useState<UpdateInfo | null>(status?.update ?? null);
  const [checking, setChecking] = useState(false);
  const app = config.app;
  const [dataDir, setDataDir] = useState<DataDirInfo | null>(null);
  const [newDir, setNewDir] = useState("");
  const [copyData, setCopyData] = useState(true);
  const [pendingRestart, setPendingRestart] = useState(false);
  useEffect(() => { api.dataDirInfo().then(setDataDir).catch(() => setDataDir(null)); }, []);
  const pickDir = async () => {
    const p = await open({ directory: true, multiple: false, title: "Папка для данных SignoreBot" });
    if (p && !Array.isArray(p)) setNewDir(p);
  };
  const applyDir = (path: string | null) => {
    const what = path ? `в «${path}»` : `в стандартную папку «${dataDir?.default ?? ""}»`;
    showConfirm(`Перенести данные ${what}?\n\n${copyData ? "Текущие настройки, медиа и резервные копии будут скопированы (старая папка останется как есть)." : "Копирование выключено: в новой папке бот начнёт с тем, что там есть (или с пустых настроек)."}\n\nПосле этого приложение перезапустится.`, async () => {
      try {
        const n = await api.dataDirSet(path, copyData);
        showNotification(`Готово, скопировано файлов: ${n}. Перезапуск…`, NOTIFICATION_TYPES.SUCCESS, 3000);
        setPendingRestart(true);
        setTimeout(() => void api.appRestart(), 1200);
      } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 8000); }
    });
  };
  const checkUpdates = async () => {
    setChecking(true);
    try {
      const u = await api.updatesCheck();
      setUpdate(u);
      showNotification(u.isNewer ? `🆕 Доступна версия ${u.latest}` : u.latest ? `У вас последняя версия (${u.current})` : "Релизов пока нет", u.isNewer ? NOTIFICATION_TYPES.WARNING : NOTIFICATION_TYPES.SUCCESS, 4000);
    } catch (e) { showNotification(`Проверка обновлений: ${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000); }
    finally { setChecking(false); }
  };

  const exportConfig = async () => {
    try {
      const path = await save({ defaultPath: "signorebot-config.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const doc = await api.configExport();
      await api.configExportWrite(path, JSON.stringify(doc, null, 2));
      showNotification("Настройки экспортированы (без токенов и ключа)", NOTIFICATION_TYPES.SUCCESS, 3000);
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 4000); }
  };
  const importConfig = async () => {
    const p = await open({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!p || Array.isArray(p)) return;
    showConfirm("Заменить текущие настройки содержимым файла?\n\nАккаунты, сетевые настройки и ключ оверлеев сохранятся. Резервная копия будет создана.", async () => {
      try { const r = await api.configImportFile(p); await reloadConfig(); showNotification(`Импортировано (формат v${r.report.fromVersion})`, NOTIFICATION_TYPES.SUCCESS, 3000); r.report.notes.forEach((n) => showNotification(`${n}`, NOTIFICATION_TYPES.INFO, 8000)); }
      catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 6000); }
    });
  };
  const regenKey = () => showConfirm("Перевыпустить ключ оверлеев?\n\nВсе URL в OBS перестанут работать — их нужно будет обновить.", async () => {
    try { await api.overlayKeyRegenerate(); showNotification("Ключ перевыпущен — обновите URL оверлеев в OBS", NOTIFICATION_TYPES.WARNING, 5000); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 4000); }
  });

  return (
    <div className="settings-tab">
      <div className="commands-header"><h2><Icon name="settings" /> Настройки</h2><p className="commands-description">Сеть, ключ доступа, поведение оверлеев, экспорт настроек.</p></div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="globe" /> Сеть</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-row">
            <div className="form-group">
              <label>Порт сервера оверлеев <Tooltip text="HTTP и WebSocket на одном порту. После изменения сервер перезапустится, URL в OBS нужно обновить." /></label>
              <input type="number" min={1024} max={65535} value={port} onChange={(e) => setPort(e.target.value)} onBlur={() => { const p = parseInt(port); if (p >= 1024 && p <= 65535 && p !== net.httpPort) setSection("network", { ...net, httpPort: p }); else setPort(String(net.httpPort)); }} />
            </div>
            <div className="form-group">
              <label>Доступ из локальной сети <Tooltip text="Нужно, если OBS работает на другом компьютере. Иначе сервер слушает только 127.0.0.1." /></label>
              <label className="toggle-label field-height">
                <span className="toggle-switch"><input type="checkbox" checked={net.allowLan} onChange={(e) => { setSection("network", { ...net, allowLan: e.target.checked }); showNotification(e.target.checked ? "Доступ из сети включён: в адресах оверлеев появится IP компьютера. Источники в OBS с адресами 127.0.0.1 продолжат работать." : "Доступ из сети выключен: адреса с IP компьютера больше не работают. Если Browser Source в OBS настроены на такой адрес — обновите их кнопкой «В OBS» на вкладке «Оверлеи», иначе оверлеи не подключатся.", NOTIFICATION_TYPES.WARNING, 9000); }} /><span className="toggle-slider"></span></span>
                <span className="toggle-text">{net.allowLan ? `включён (IP: ${status?.server.lanIp ?? "…"})` : "выключен"}</span>
              </label>
            </div>
          </div>
          <div className="form-group">
            <label>Ключ доступа к оверлеям <Tooltip text="Входит в URL оверлеев. Без него страницы, медиа и WebSocket недоступны." /></label>
            <div className="form-row"><input type="text" value={net.overlayKey} readOnly style={{ fontFamily: "var(--font-mono)" }} /><button onClick={regenKey} style={{ flex: "0 0 auto" }}><Icon name="key"  /> Перевыпустить</button></div>
          </div>
          <div className="form-hint">Текущий адрес: {status?.server.running ? `http://${status.server.address}` : "сервер не запущен"}{status?.server.error && ` — ${status.server.error}`}</div>
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="clapperboard" /> Поведение оверлея</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-row">
            <div className="form-group"><label>Пауза между элементами очереди, мс</label><input type="number" min={0} max={30000} step={100} value={ov.pauseBetweenMs} onChange={(e) => setSection("overlaySettings", { ...ov, pauseBetweenMs: Math.max(0, parseInt(e.target.value) || 0) })} /></div>
            <div className="form-group"><label>Показ картинки по умолчанию, с</label><input type="number" min={1} max={600} step={0.5} value={ov.imageDurationSec} onChange={(e) => setSection("overlaySettings", { ...ov, imageDurationSec: Math.max(0.5, parseFloat(e.target.value) || 10) })} /></div>
            <div className="form-group"><label>Антиспам, мс <Tooltip text="Тот же файл от того же пользователя в этом окне пропускается (0 — выключено). Разные пользователи не ограничиваются — их медиа встают в очередь." /></label><input type="number" min={0} max={60000} step={100} value={ov.antispamWindowMs} onChange={(e) => setSection("overlaySettings", { ...ov, antispamWindowMs: Math.max(0, parseInt(e.target.value) || 0) })} /></div>
          </div>
          <div className="form-hint">Изменения применяются к оверлеям при их следующем подключении (перезагрузите Browser Source).</div>
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="plug" /> Twitch-приложение</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-group">
            <label>Client ID <Tooltip text="Публичный идентификатор приложения из dev.twitch.tv (тип «Public», Device Code Flow). Меняйте только если создали своё приложение." /></label>
            <div className="form-row"><input type="text" value={clientId} onChange={(e) => setClientId(e.target.value)} style={{ fontFamily: "var(--font-mono)" }} /><button onClick={() => { if (clientId.trim() && clientId.trim() !== config.twitch.clientId) { setSection("twitch", { clientId: clientId.trim() }); showNotification("Client ID сохранён — переавторизуйте аккаунты", NOTIFICATION_TYPES.WARNING, 4000); } }} style={{ flex: "0 0 auto" }}>Сохранить</button></div>
          </div>
          <div className="form-hint">Токены хранятся: {status?.secretsBackend === "keyring" ? "в системном хранилище (keyring)" : "в файле secrets.json (системное хранилище недоступно)"}.</div>
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="new-item" /> Обновления</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-group">
            <label>Репозиторий с релизами <Tooltip text="GitHub-репозиторий, где публикуются версии. Авторы форков могут указать свой — или прямую ссылку на JSON в формате страницы «последний релиз» GitHub, если релизы лежат на своём сервере." /></label>
            <div className="form-row">
              <input type="text" value={repoUrl} onChange={(e) => setRepoUrl(e.target.value)} onBlur={() => { const v = repoUrl.trim(); if (v && v !== upd.repoUrl) setSection("updates", { ...upd, repoUrl: v }); }} style={{ fontFamily: "var(--font-mono)" }} />
              <button onClick={() => void checkUpdates()} disabled={checking} style={{ flex: "0 0 auto" }}><Icon name="refresh" className={checking ? "spinning" : ""} /> Проверить</button>
            </div>
          </div>
          <label className="toggle-label">
            <span className="toggle-switch"><input type="checkbox" checked={upd.checkOnStart} onChange={(e) => setSection("updates", { ...upd, checkOnStart: e.target.checked })} /><span className="toggle-slider"></span></span>
            <span className="toggle-text">Проверять при запуске и раз в 12 часов</span>
          </label>
          {update && (
            <div className={`status-hint mt-3 ${update.isNewer ? "status-update-available" : ""}`}>
              Текущая версия: <strong>{update.current}</strong>{update.latest && <> · последний релиз: <strong>{update.latest}</strong>{update.publishedAt && ` (${new Date(update.publishedAt).toLocaleDateString("ru-RU")})`}</>}
              {update.isNewer && update.url && <div className="mt-2 flex gap-2 items-center"><button className="primary" onClick={() => void openUrl(update.url!)}><Icon name="download"  /> Скачать {update.latest}</button>
                {update.assets.slice(0, 4).map((a) => <button key={a.url} className="small" onClick={() => void openUrl(a.url)} title={a.name}>{a.name.length > 28 ? a.name.slice(0, 26) + "…" : a.name}</button>)}</div>}
              {!update.isNewer && update.latest && <div className="text-success mt-1">У вас последняя версия.</div>}
              {!update.latest && <div className="mt-1">В репозитории пока нет релизов — проверено {new Date(update.checkedAt).toLocaleTimeString("ru-RU")}.</div>}
              {update.notes && update.isNewer && <pre className="mt-2" style={{ whiteSpace: "pre-wrap", fontFamily: "inherit", color: "var(--text-secondary)", maxHeight: 200, overflow: "auto" }}>{update.notes}</pre>}
            </div>
          )}
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="app-window" /> Окно и трей</h3></div></div>
        <div style={{ padding: 20 }}>
          <label className="toggle-label">
            <span className="toggle-switch"><input type="checkbox" checked={app.closeToTray} onChange={(e) => setSection("app", { ...app, closeToTray: e.target.checked })} /><span className="toggle-slider"></span></span>
            <span className="toggle-text">Кнопка «Закрыть» сворачивает в трей, бот продолжает работать</span>
            <Tooltip text="Выключите — и закрытие окна полностью завершит приложение вместе с ботом. Выйти из трея можно и через его меню." />
          </label>
          <div className="form-hint">{app.closeToTray ? "Сейчас: окно прячется в трей; выход — через меню иконки в трее." : "Сейчас: закрытие окна останавливает бота и завершает приложение."}</div>
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="info" /> Уведомления</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-row">
            <div className="form-group"><label>Сколько держать на экране, с <Tooltip text="Нижняя планка: короткие сообщения («сохранено», «скопировано») живут столько; ошибки и предупреждения — дольше, если им так задано." /></label><input type="number" min={1} max={120} step={1} style={{ maxWidth: 200 }} value={app.notificationSeconds} disabled={app.notificationsSticky} onChange={(e) => setSection("app", { ...app, notificationSeconds: Math.min(120, Math.max(1, parseFloat(e.target.value) || 6)) })} /></div>
          </div>
          <label className="toggle-label" style={{ marginTop: 8 }}>
            <span className="toggle-switch"><input type="checkbox" checked={app.notificationsSticky} onChange={(e) => setSection("app", { ...app, notificationsSticky: e.target.checked })} /><span className="toggle-slider"></span></span>
            <span className="toggle-text">Не исчезать сами — только по крестику</span>
          </label>
          <div className="form-hint">{app.notificationsSticky ? "Сейчас: уведомления висят, пока не закроете; на экране одновременно не больше шести, старые вытесняются." : `Сейчас: уведомление видно не меньше ${app.notificationSeconds} с, затем исчезает само.`}</div>
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="eye" /> Масштаб панели</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-group">
            <label>Размер текста и значков <Tooltip text="Как Ctrl+плюс в браузере: крупнее становится всё сразу — текст, значки, поля. На маленьком окне часть карточек может переноситься на две строки." /></label>
            <select value={String(app.uiZoom || 100)} onChange={(e) => setSection("app", { ...app, uiZoom: parseInt(e.target.value) || 100 })} style={{ maxWidth: 240 }}>
              {[90, 100, 110, 125, 150].map((z) => <option key={z} value={z}>{z === 100 ? "100% — как есть" : `${z}%`}</option>)}
            </select>
          </div>
          <div className="form-hint">Применяется сразу и запоминается.</div>
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="folder-open" /> Папка данных</h3></div></div>
        <div style={{ padding: 20 }}>
          <div className="form-group">
            <label>Текущая папка <Tooltip text="Здесь лежат config.json, медиа, резервные копии конфига и логи. Токены Twitch — в системном хранилище (или в secrets.json, если оно недоступно)." /></label>
            <div className="form-row">
              <input type="text" value={dataDir?.current ?? "…"} readOnly style={{ fontFamily: "var(--font-mono)" }} />
              <button onClick={() => void api.openDataDir()} style={{ flex: "0 0 auto" }}><Icon name="folder-open"  /> Открыть</button>
            </div>
            {dataDir?.source === "pointer" && <div className="form-hint">Нестандартная папка; указатель на неё лежит в «{dataDir.default}».</div>}
            {dataDir?.source === "env" && <div className="form-hint">Папка задана переменной окружения SIGNOREBOT_DATA_DIR — из панели её не сменить.</div>}
          </div>
          {dataDir && dataDir.source !== "env" && (
            <>
              <div className="form-group">
                <label>Перенести в другую папку <Tooltip text="Например, на другой диск, если на системном мало места. Старая папка не удаляется — её можно убрать вручную после проверки." /></label>
                <div className="form-row">
                  <input type="text" value={newDir} onChange={(e) => setNewDir(e.target.value)} placeholder="Полный путь к папке (выбирайте пустую папку)" style={{ fontFamily: "var(--font-mono)" }} />
                  <button onClick={() => void pickDir()} style={{ flex: "0 0 auto" }}><Icon name="folder-open"  /> Выбрать…</button>
                </div>
              </div>
              <label className="toggle-label" style={{ marginBottom: 12 }}>
                <span className="toggle-switch"><input type="checkbox" checked={copyData} onChange={(e) => setCopyData(e.target.checked)} /><span className="toggle-slider"></span></span>
                <span className="toggle-text">Скопировать текущие данные в новую папку</span>
              </label>
              <div className="flex gap-2">
                <button className="primary" disabled={!newDir.trim() || pendingRestart} onClick={() => applyDir(newDir.trim())}><Icon name="redo"  /> Перенести и перезапустить</button>
                {dataDir.source === "pointer" && <button disabled={pendingRestart} onClick={() => applyDir(null)}>Вернуть стандартную папку</button>}
              </div>
            </>
          )}
        </div>
      </div>

      <div className="card mb-4"><div className="card-header" style={{ cursor: "default" }}><div className="card-title"><h3><Icon name="package" /> Экспорт и импорт</h3></div></div>
        <div style={{ padding: 20 }} className="flex gap-2">
          <button onClick={() => void exportConfig()}><Icon name="download"  /> Экспортировать настройки</button>
          <button onClick={() => void importConfig()}><Icon name="upload"  /> Импортировать из файла</button>
        </div>
      </div>
    </div>
  );
}
