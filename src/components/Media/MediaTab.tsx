// Каталог медиа: список, добавление, удаление, уборка неиспользуемых.

import Icon, { type IconName } from "../Icon";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errText, type MediaFile } from "../../api";
import Modal from "../Common/Modal";
import { formatSize } from "../../api/defaults";
import { pickAndImport, useMediaFiles } from "../Common/MediaEditor";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "../Common/MediaEditor.css";

export default function MediaTab() {
  const { files, reload } = useMediaFiles();
  const { showNotification, showConfirm } = useNotification();
  const [q, setQ] = useState("");
  const [kind, setKind] = useState<"all" | "video" | "audio" | "image">("all");
  const [onlyUnused, setOnlyUnused] = useState(false);
  const [preview, setPreview] = useState<MediaFile | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  // Громкость плеера: у нативных контролов WebKit нет ползунка, только mute.
  const [volume, setVolume] = useState(() => { try { const v = parseFloat(localStorage.getItem("sb.previewVolume") ?? ""); return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 1; } catch { return 1; } });
  const playerRef = useRef<HTMLMediaElement | null>(null);
  useEffect(() => {
    if (playerRef.current) playerRef.current.volume = volume;
    try { localStorage.setItem("sb.previewVolume", String(volume)); } catch { /* приватный режим */ }
  }, [volume, previewUrl]);
  useEffect(() => {
    if (!preview) { setPreviewUrl(null); return; }
    let alive = true;
    api.mediaUrl(preview.name).then((u) => { if (alive) setPreviewUrl(u); }).catch(() => setPreviewUrl(null));
    return () => { alive = false; };
  }, [preview]);
  const list = useMemo(() => files.filter((f) => (kind === "all" || f.kind === kind) && (!onlyUnused || !f.used) && f.name.toLowerCase().includes(q.toLowerCase())), [files, kind, onlyUnused, q]);
  const unused = files.filter((f) => !f.used);
  const unusedSize = unused.reduce((a, f) => a + f.size, 0);
  const icon = (k: string): IconName => (k === "video" ? "clapperboard" : k === "audio" ? "audio-note" : k === "image" ? "image" : "document");

  const del = (name: string) => showConfirm(`Удалить файл "${name}"?`, async () => {
    try { await api.mediaDelete(name); showNotification(`Файл "${name}" удалён`, NOTIFICATION_TYPES.WARNING, 2000); await reload(); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  });
  const cleanup = () => showConfirm(`Удалить все неиспользуемые файлы (${unused.length} шт., ${formatSize(unusedSize)})?\n\nЭто файлы, на которые не ссылается ни одна команда, награда, событие или таймер.`, async () => {
    try { const n = await api.mediaDeleteUnused(); showNotification(`Удалено файлов: ${n}`, NOTIFICATION_TYPES.SUCCESS, 3000); await reload(); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  });

  return (
    <div className="media-tab">
      <div className="commands-header">
        <h2><Icon name="filmstrip" /> Медиа</h2>
        <p className="commands-description">Все файлы, доступные для реакций. Всего: {files.length} ({formatSize(files.reduce((a, f) => a + f.size, 0))}), неиспользуемых: {unused.length} ({formatSize(unusedSize)}).</p>
        <div className="flex gap-2 mt-3">
          <button className="primary" onClick={() => void pickAndImport(null, showNotification).then((a) => { if (a.length) void reload(); })}><Icon name="add"  /> Добавить файлы</button>
          <button className="warning" onClick={cleanup} disabled={unused.length === 0}><Icon name="clean"  /> Удалить неиспользуемые</button>
        </div>
      </div>
      <div className="file-browser media-tab-browser">
        <div className="file-browser-header">
          <div className="flex gap-2 items-center">
            <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)} style={{ width: "auto" }}>
              <option value="all">Все типы</option><option value="video">Видео</option><option value="audio">Аудио</option><option value="image">Картинки</option>
            </select>
            <label className="toggle-label"><input type="checkbox" checked={onlyUnused} onChange={(e) => setOnlyUnused(e.target.checked)} style={{ width: "auto" }} /> только неиспользуемые</label>
          </div>
          <div className="file-search">
            <Icon name="search" className="search-icon" />
            <input type="text" placeholder="Поиск файлов..." value={q} onChange={(e) => setQ(e.target.value)} className="file-search-input" />
            {q && <button className="search-clear-btn" onClick={() => setQ("")}><Icon name="close" /> </button>}
          </div>
        </div>
        {list.length === 0 ? <p className="empty-files">{files.length === 0 ? "Нет файлов" : "Ничего не найдено"}</p> : (
          <div className="files-grid">
            {list.map((f) => (
              <div key={f.name} className="file-item" onClick={() => setPreview(f)} title="Открыть предпросмотр">
                <span className="file-icon"><Icon name={icon(f.kind)} /></span>
                <span className="file-name" title={f.name}>{f.name}</span>
                {!f.used && <span className="badge badge-warning" title="Не используется ни одной реакцией">сирота</span>}
                <span className="file-size">{formatSize(f.size)}</span>
                <button className="delete-file-btn" title="Удалить" onClick={() => del(f.name)}><Icon name="delete"  /></button>
              </div>
            ))}
          </div>
        )}
      </div>
      <Modal isOpen={!!preview} onClose={() => setPreview(null)} title={preview ? <><Icon name={icon(preview.kind)} /> {preview.name}</> : ""} size="large">
        {preview && (
          <div className="media-preview-modal">
            {previewUrl && preview.kind === "image" && <img src={previewUrl} alt="" />}
            {previewUrl && preview.kind === "video" && <video ref={(el) => { playerRef.current = el; }} src={previewUrl} controls autoPlay onLoadedMetadata={(e) => { e.currentTarget.volume = volume; }} />}
            {previewUrl && preview.kind === "audio" && <audio ref={(el) => { playerRef.current = el; }} src={previewUrl} controls autoPlay onLoadedMetadata={(e) => { e.currentTarget.volume = volume; }} />}
            {previewUrl && (preview.kind === "video" || preview.kind === "audio") && (
              <div className="media-preview-volume">
                <button className="small" onClick={() => setVolume(volume > 0 ? 0 : 1)} title={volume > 0 ? "Выключить звук" : "Включить звук"}>{volume > 0 ? <Icon name="volume-on"  /> : <Icon name="volume-muted"  />}</button>
                <input type="range" min={0} max={100} value={Math.round(volume * 100)} onChange={(e) => setVolume(parseInt(e.target.value) / 100)} />
                <span className="media-preview-volume-value">{Math.round(volume * 100)}%</span>
              </div>
            )}
            {preview.kind === "unknown" && <p className="text-muted">Неизвестный тип файла — предпросмотр недоступен.</p>}
            <div className="form-hint" style={{ marginTop: 12 }}>{formatSize(preview.size)} · {new Date(preview.modified).toLocaleString("ru-RU")} · {preview.used ? "используется в реакциях" : "не используется (сирота)"}</div>
          </div>
        )}
      </Modal>
    </div>
  );
}
