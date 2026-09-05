import Icon from "../Icon";
import TestButton from "../Common/TestButton";
import { useState } from "react";
import type { Command, Response } from "../../api";
import { defaultCommand } from "../../api/defaults";
import { useAppState } from "../../state/AppState";
import AliasEditor from "../Common/AliasEditor";
import Modal, { ModalActions } from "../Common/Modal";
import { Hint, hintAliases, hintCooldown, hintReply, hintOverlay, hintOverlayAll, hintPermissions, hintReaction, hintStatus } from "../Common/hints";
import ResponseEditor from "../Common/ResponseEditor";
import { VariableBadges } from "../Common/VariableBadge";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import PermissionsSelector from "./PermissionsSelector";
import Tooltip from "../Tooltip";
import "./CommandsTab.css";
import "./CommandEditor.css";

export function reactionBadge(r: Response) {
  const c = r.chat.enabled, m = r.media.enabled;
  if (!c && !m) return <><Icon name="sleep" /> Нет реакции</>;
  if (c && !m) return <><Icon name="chat" /> Текст</>;
  if (!c && m) return <><Icon name="clapperboard" /> Медиа</>;
  return <><span className="badge-icons"><Icon name="chat" /><Icon name="clapperboard" /></span> Текст + Медиа</>;
}

function CommandEditor({ initial, isNew, all, onSave }: { initial: Command; isNew: boolean; all: Command[]; onSave: (c: Command) => void }) {
  const { config } = useAppState();
  const [cmd, setCmd] = useState<Command>(initial);
  const name = cmd.name.trim().replace(/^!+/, "").toLowerCase();
  const empty = !name;
  return (
    <div className="command-editor">
      <ModalActions>
        <TestButton response={cmd.response} vars={{ user: "TestUser", target: "TestUser", message: "тест" }} />
        <button onClick={() => onSave({ ...cmd, name })} className="save-command-btn primary" disabled={empty} title={empty ? "Введите название команды" : ""}>
          {isNew ? <><Icon name="add"  /> Создать команду</> : <><Icon name="save"  /> Сохранить</>}
        </button>
      </ModalActions>
      <div className="command-editor-header">
        <div className="form-row" style={{ alignItems: "flex-start" }}>
          <div className="command-name-section" style={{ flex: 3 }}>
            <label>Название команды (без !)</label>
            <div className="command-name-input-wrapper">
              <span className="command-name-prefix">!</span>
              <input type="text" value={cmd.name} onChange={(e) => setCmd({ ...cmd, name: e.target.value.replace(/^!+/, "").replace(/\s+/g, "") })} placeholder="название (без пробелов)" className="command-name-input" autoFocus={isNew} />
            </div>
            {!empty && <div className="command-preview"><span className="preview-label">Будет доступна как:</span><code className="command-preview-name">!{name}</code></div>}
          </div>
          <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
            <label><Icon name="stopwatch" /> Кулдаун, с <Tooltip text="Минимальная пауза между срабатываниями команды для всех зрителей вместе. 0 — без ограничения." /></label>
            <input type="number" min={0} max={86400} value={cmd.cooldownSec} onChange={(e) => setCmd({ ...cmd, cooldownSec: Math.max(0, parseInt(e.target.value) || 0) })} />
          </div>
          <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
            <label><Icon name="user" /> На одного зрителя, с <Tooltip text="Пауза между вызовами одним и тем же зрителем; остальные в это время могут вызывать команду. 0 — без ограничения. Работает вместе с общим кулдауном." /></label>
            <input type="number" min={0} max={86400} value={cmd.cooldownUserSec} onChange={(e) => setCmd({ ...cmd, cooldownUserSec: Math.max(0, parseInt(e.target.value) || 0) })} />
          </div>
        </div>
        <label className="toggle-label" style={{ marginTop: 10 }}>
          <span className="toggle-switch"><input type="checkbox" checked={cmd.reply} onChange={(e) => setCmd({ ...cmd, reply: e.target.checked })} /><span className="toggle-slider"></span></span>
          <span className="toggle-text">Отвечать реплаем на сообщение зрителя</span>
          <Tooltip text="Текст в чат уходит как ответ на сообщение зрителя — в чате Twitch видно «в ответ @зритель». Медиа это не касается." />
        </label>
      </div>
      <PermissionsSelector value={cmd.permissions} onChange={(permissions) => setCmd({ ...cmd, permissions })} />
      <ResponseEditor
        value={cmd.response}
        onChange={(response) => setCmd({ ...cmd, response })}
        overlays={config.overlays}
        variables={["user", "target", "message"]}
        extraTab={{ label: <><Icon name="lightning" /> Алиасы</>, content: <AliasEditor value={cmd.aliases} onChange={(aliases) => setCmd({ ...cmd, aliases })} allCommands={all} currentId={cmd.id} currentName={name} /> }}
      />
    </div>
  );
}

export default function CommandsTab() {
  const { config, setSection } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const [editing, setEditing] = useState<{ cmd: Command; isNew: boolean } | null>(null);
  const commands = config.commands;

  const save = (c: Command) => {
    const clash = commands.find((x) => x.id !== c.id && (x.name === c.name || x.aliases.includes(c.name) || c.aliases.some((a) => a === x.name || x.aliases.includes(a))));
    if (clash) return void showNotification(`Имя или алиас уже занят командой !${clash.name}`, NOTIFICATION_TYPES.ERROR, 3000);
    const exists = commands.some((x) => x.id === c.id);
    setSection("commands", exists ? commands.map((x) => (x.id === c.id ? c : x)) : [...commands, c]);
    setEditing(null);
    showNotification(exists ? `Команда !${c.name} сохранена` : `Команда !${c.name} создана`, NOTIFICATION_TYPES.SUCCESS, 2000);
  };
  const toggle = (c: Command) => {
    setSection("commands", commands.map((x) => (x.id === c.id ? { ...x, enabled: !x.enabled } : x)));
    showNotification(`Команда !${c.name} ${!c.enabled ? "включена" : "выключена"}`, NOTIFICATION_TYPES.INFO, 1500);
  };
  const remove = (c: Command) =>
    showConfirm(`Удалить команду !${c.name}?\n\nЭто действие нельзя отменить.`, () => {
      setSection("commands", commands.filter((x) => x.id !== c.id));
      showNotification(`Команда !${c.name} удалена`, NOTIFICATION_TYPES.WARNING, 2000);
    });

  return (
    <div className="commands-tab">
      <div className="commands-header">
        <h2><Icon name="robot" /> Команды чата</h2>
        <p className="commands-description">
          Команды, которые бот выполняет в чате. Переменные:
          <VariableBadges className="inline-variable-list" variables={["user", "target", "message"]} />
        </p>
        <button className="create-command-btn" onClick={() => setEditing({ cmd: defaultCommand(), isNew: true })}><Icon name="add"  /> Создать команду</button>
      </div>
      <div className="commands-list">
        {commands.length === 0 && <div className="empty-commands"><p><Icon name="inbox-empty" /> Команды не созданы</p><p className="hint">Нажмите «Создать команду»</p></div>}
        {commands.map((c) => {
          const ov = c.response.media.enabled && c.response.media.overlay ? config.overlays.find((o) => o.id === c.response.media.overlay) : null;
          return (
            <div key={c.id} className={`command-card ${!c.enabled ? "disabled" : ""}`}>
              <div className="command-card-header">
                <div className="command-title">
                  <span className="command-name">!{c.name}</span>
                  <Hint text={hintReaction({ kind: "command", name: c.name }, c.response)}><span className="command-type-badge">{reactionBadge(c.response)}</span></Hint>
                  {c.aliases.length > 0 && <Hint text={hintAliases(c.name, c.aliases)}><span className="command-aliases-badge"><Icon name="lightning" /> {c.aliases.map((a) => `!${a}`).join(", ")}</span></Hint>}
                  {c.permissions.length > 0 && <Hint text={hintPermissions(c.name, c.permissions)}><span className="permissions-badge"><Icon name="lock" /> {c.permissions.length}</span></Hint>}
                  {(c.cooldownSec > 0 || c.cooldownUserSec > 0) && <Hint text={hintCooldown(c.name, c.cooldownSec, c.cooldownUserSec)}><span className="command-type-badge"><Icon name="stopwatch" /> {[c.cooldownSec > 0 ? `${c.cooldownSec} с` : null, c.cooldownUserSec > 0 ? `${c.cooldownUserSec} с/зритель` : null].filter(Boolean).join(" · ")}</span></Hint>}
                  {c.reply && <Hint text={hintReply(c.name)}><span className="command-type-badge"><Icon name="chat" /> реплай</span></Hint>}
                  {ov && <Hint text={hintOverlay(ov)}><span className="overlay-badge"><Icon name="overlay-screen" /> {ov.name}</span></Hint>}
                  {!ov && c.response.media.enabled && <Hint text={hintOverlayAll(config.overlays)}><span className="overlay-badge all-overlays"><Icon name="broadcast" /> Все оверлеи</span></Hint>}
                </div>
                <div className="command-actions">
                  <Hint text={hintStatus({ kind: "command", name: c.name }, c.enabled)}><button onClick={() => toggle(c)} className={`status-toggle-btn ${c.enabled ? "on" : "off"}`}><Icon name="power"  /></button></Hint>
                  <button onClick={() => setEditing({ cmd: c, isNew: false })} className="edit-btn" title="Редактировать"><Icon name="edit"  /></button>
                  <button onClick={() => remove(c)} className="delete-btn" title="Удалить"><Icon name="delete"  /></button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <Modal isOpen={!!editing} onClose={() => setEditing(null)} title={editing?.isNew ? "Создание новой команды" : `Редактирование команды !${editing?.cmd.name ?? ""}`} size="xlarge">
        {editing && <CommandEditor key={editing.cmd.id} initial={editing.cmd} isNew={editing.isNew} all={commands} onSave={save} />}
      </Modal>
    </div>
  );
}
