import Icon from "../Icon";
import { useState } from "react";
import type { Command } from "../../api";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "./ResponseEditor.css";

export default function AliasEditor({ value, onChange, allCommands, currentId, currentName }: { value: string[]; onChange: (v: string[]) => void; allCommands: Command[]; currentId: string; currentName: string }) {
  const { showNotification } = useNotification();
  const [text, setText] = useState("");
  const add = () => {
    const alias = text.trim().replace(/^!+/, "").replace(/\s+/g, "").toLowerCase();
    if (!alias) return;
    if (value.includes(alias)) return void showNotification("Такой алиас уже существует", NOTIFICATION_TYPES.WARNING, 2000);
    if (alias === currentName.toLowerCase()) return void showNotification("Алиас не может совпадать с именем команды", NOTIFICATION_TYPES.WARNING, 2000);
    const clash = allCommands.find((c) => c.id !== currentId && (c.name === alias || c.aliases.includes(alias)));
    if (clash) return void showNotification(`«!${alias}» уже занят командой !${clash.name}`, NOTIFICATION_TYPES.ERROR, 3000);
    onChange([...value, alias]);
    setText("");
  };
  return (
    <div className="aliases-editor">
      <div className="aliases-header">
        <h4><Icon name="lightning" /> Алиасы команды</h4>
        <p className="aliases-description">Дополнительные имена той же команды. Не могут совпадать с другими командами и их алиасами.</p>
      </div>
      <div className="aliases-list">
        {value.length === 0 ? (
          <div className="empty-aliases"><p><Icon name="inbox-empty" /> У команды нет алиасов</p><p className="hint">Добавьте алиас, чтобы команду можно было вызвать по нескольким именам</p></div>
        ) : value.map((a) => (
          <div key={a} className="alias-item">
            <span className="alias-name">!{a}</span>
            <button onClick={() => onChange(value.filter((x) => x !== a))} className="remove-alias-btn" title="Удалить алиас"><Icon name="delete"  /></button>
          </div>
        ))}
      </div>
      <div className="add-alias-form">
        <input type="text" value={text} onChange={(e) => setText(e.target.value.replace(/^!+/, ""))} placeholder="Название алиаса (без !)" onKeyDown={(e) => e.key === "Enter" && add()} className="alias-input" />
        <button onClick={add} className="add-alias-btn"><Icon name="add"  /> Добавить алиас</button>
      </div>
      <div className="aliases-warning"><Icon name="warning" /> Алиас работает только если команда включена</div>
    </div>
  );
}
