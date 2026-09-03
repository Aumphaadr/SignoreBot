import Icon, { type IconName } from "../Icon";
import { useState } from "react";
import type { Note, NoteStatus } from "../../api";
import { newId } from "../../api/defaults";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "./NotesTab.css";

const LABEL: Record<NoteStatus, [IconName, string]> = { active: ["pin", "В процессе"], done: ["success-badge", "Выполнено"], cancelled: ["error-badge", "Отменено"] };

export default function NotesTab() {
  const { config, setSection } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const notes = config.notes;
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editText, setEditText] = useState("");
  const [creating, setCreating] = useState(false);
  const [newText, setNewText] = useState("");
  const now = () => new Date().toISOString();
  const save = (n: Note[]) => setSection("notes", n);

  const create = () => {
    const t = newText.trim();
    if (!t) return void showNotification("Заметка не может быть пустой", NOTIFICATION_TYPES.WARNING, 2000);
    save([{ id: newId("note"), text: t, status: "active", createdAt: now(), updatedAt: now() }, ...notes]);
    setCreating(false); setNewText("");
    showNotification("Заметка создана", NOTIFICATION_TYPES.SUCCESS, 1500);
  };
  const saveEdit = () => {
    const t = editText.trim();
    if (!t) return void showNotification("Заметка не может быть пустой", NOTIFICATION_TYPES.WARNING, 2000);
    save(notes.map((n) => (n.id === editingId ? { ...n, text: t, updatedAt: now() } : n)));
    setEditingId(null);
    showNotification("Заметка обновлена", NOTIFICATION_TYPES.SUCCESS, 1500);
  };
  const setStatus = (id: string, status: NoteStatus) => save(notes.map((n) => (n.id === id ? { ...n, status, updatedAt: now() } : n)));
  const remove = (n: Note) => showConfirm(`Удалить заметку?\n\n"${n.text.length > 40 ? n.text.slice(0, 40) + "..." : n.text}"`, () => { save(notes.filter((x) => x.id !== n.id)); if (editingId === n.id) setEditingId(null); showNotification("Заметка удалена", NOTIFICATION_TYPES.WARNING, 1500); });
  const removeAll = (status: NoteStatus) => {
    const count = notes.filter((n) => n.status === status).length;
    if (!count) return void showNotification("Нечего удалять", NOTIFICATION_TYPES.INFO, 1500);
    showConfirm(`Удалить все заметки со статусом «${LABEL[status][1]}»?\n\nБудет удалено: ${count}`, () => { save(notes.filter((n) => n.status !== status)); showNotification(`Удалено ${count} заметок`, NOTIFICATION_TYPES.WARNING, 2000); });
  };
  const fmt = (iso: string) => new Date(iso).toLocaleDateString("ru-RU", { day: "2-digit", month: "2-digit", year: "2-digit", hour: "2-digit", minute: "2-digit" });

  const card = (n: Note) => (
    <div key={n.id} className={`note-card ${n.status}`}>
      {editingId === n.id ? (
        <>
          <textarea className="note-textarea" value={editText} onChange={(e) => setEditText(e.target.value)} rows={4} autoFocus />
          <div className="note-editor-actions"><button className="note-save-btn" onClick={saveEdit}><Icon name="save"  /> Сохранить</button><button className="note-cancel-btn" onClick={() => setEditingId(null)}><Icon name="close"  /> Отмена</button></div>
        </>
      ) : (
        <>
          <div className="note-content"><pre className="note-text">{n.text}</pre></div>
          <div className="note-footer">
            <span className="note-date">{fmt(n.updatedAt || n.createdAt)}</span>
            <div className="note-actions">
              <div className="note-status-buttons">
                {n.status !== "active" && <button className="note-status-btn status-active" onClick={() => setStatus(n.id, "active")} title="В процессе"><Icon name="pin"  /></button>}
                {n.status !== "done" && <button className="note-status-btn status-done" onClick={() => setStatus(n.id, "done")} title="Выполнено"><Icon name="check"  /></button>}
                {n.status !== "cancelled" && <button className="note-status-btn status-cancelled" onClick={() => setStatus(n.id, "cancelled")} title="Отменено"><Icon name="close"  /></button>}
              </div>
              {n.status === "active" && <button className="note-action-btn edit" onClick={() => { setEditingId(n.id); setEditText(n.text); setCreating(false); }} title="Редактировать"><Icon name="edit"  /></button>}
              <button className="note-action-btn delete" onClick={() => remove(n)} title="Удалить"><Icon name="delete"  /></button>
            </div>
          </div>
        </>
      )}
    </div>
  );
  const by = (s: NoteStatus) => notes.filter((n) => n.status === s);

  return (
    <div className="notes-tab">
      <div className="notes-header"><h2><Icon name="edit" /> Заметки</h2><p className="notes-description">Быстрые записи для стрима: идеи, задачи, напоминания.</p></div>
      {!creating && <button className="create-note-btn" onClick={() => { setCreating(true); setNewText(""); setEditingId(null); }}><Icon name="add"  /> Новая заметка</button>}
      {creating && (
        <div className="note-editor-card creating">
          <div className="note-editor-header"><h3><Icon name="edit" /> Новая заметка</h3></div>
          <textarea className="note-textarea" value={newText} onChange={(e) => setNewText(e.target.value)} placeholder="Что нужно запомнить?..." rows={4} autoFocus />
          <div className="note-editor-actions"><button className="note-save-btn" onClick={create}><Icon name="save"  /> Сохранить</button><button className="note-cancel-btn" onClick={() => setCreating(false)}><Icon name="close"  /> Отмена</button></div>
        </div>
      )}
      {notes.length === 0 && !creating && <div className="empty-notes"><p><Icon name="inbox-empty" /> Заметок пока нет</p><p className="hint">Нажмите «Новая заметка», чтобы создать первую</p></div>}
      {by("active").length > 0 && <div className="notes-section"><h3 className="notes-section-title"><Icon name="pin" /> В процессе ({by("active").length})</h3><div className="notes-list">{by("active").map(card)}</div></div>}
      {by("done").length > 0 && <div className="notes-section done-section"><div className="notes-section-header"><h3 className="notes-section-title"><Icon name="success-badge" /> Выполнено ({by("done").length})</h3><button className="bulk-delete-btn" onClick={() => removeAll("done")}><Icon name="delete"  /> Удалить все выполненные</button></div><div className="notes-list">{by("done").map(card)}</div></div>}
      {by("cancelled").length > 0 && <div className="notes-section cancelled-section"><div className="notes-section-header"><h3 className="notes-section-title"><Icon name="error-badge" /> Отменено ({by("cancelled").length})</h3><button className="bulk-delete-btn" onClick={() => removeAll("cancelled")}><Icon name="delete"  /> Удалить все отменённые</button></div><div className="notes-list">{by("cancelled").map(card)}</div></div>}
    </div>
  );
}
