import Icon, { type IconName } from "../Icon";
import { useState } from "react";
import Tooltip from "../Tooltip";

const ROLES: [string, IconName, string][] = [["broadcaster", "owner-crown", "Стример"], ["moderators", "moderator-shield", "Модераторы"], ["vips", "vip-star", "VIP"], ["subscribers", "television", "Подписчики"]];

/** Кто может вызывать команду. «Все» исключает остальные; роли и «Выбранные»
 *  (ручной список логинов) сочетаются между собой. Пустой список = все. */
export default function PermissionsSelector({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  const users = value.filter((v) => v.startsWith("user:"));
  const [listOpen, setListOpen] = useState(users.length > 0);
  const [user, setUser] = useState("");
  const isAll = value.length === 0 && !listOpen;
  const toggleRole = (r: string) => onChange(value.includes(r) ? value.filter((x) => x !== r) : [...value, r]);
  const toggleList = () => {
    if (listOpen) { setListOpen(false); onChange(value.filter((v) => !v.startsWith("user:"))); } else setListOpen(true);
  };
  const setAll = () => { setListOpen(false); onChange([]); };
  const addUser = () => {
    const u = user.trim().toLowerCase().replace(/^@/, "");
    if (u && !value.includes(`user:${u}`)) onChange([...value, `user:${u}`]);
    setUser("");
  };
  return (
    <div className="permissions-selector">
      <label><Icon name="lock" /> Кто может вызывать <Tooltip text="«Все» — любой зритель. Роли и «Выбранные» (список логинов) можно сочетать: например, стример + модераторы + пара ников." /></label>
      <div className="role-buttons">
        {ROLES.map(([r, ic, l]) => <button key={r} type="button" className={`role-btn ${value.includes(r) ? "active" : ""}`} onClick={() => toggleRole(r)}><Icon name={ic} /> {l}</button>)}
        <button type="button" className={`role-btn ${listOpen ? "active" : ""}`} onClick={toggleList}><Icon name="target" /> Выбранные{users.length > 0 ? ` (${users.length})` : ""}</button>
        <button type="button" className={`role-btn ${isAll ? "active" : ""}`} onClick={setAll}><Icon name="globe" /> Все</button>
      </div>
      {listOpen && (
        <div className="users-section">
          {users.length > 0 && (
            <div className="users-list">
              {users.map((v) => (
                <div key={v} className="user-tag"><Icon name="user" /> {v.slice(5)}<button onClick={() => onChange(value.filter((x) => x !== v))} className="remove-user">×</button></div>
              ))}
            </div>
          )}
          <div className="add-user">
            <input type="text" value={user} onChange={(e) => setUser(e.target.value)} placeholder="Логин пользователя" onKeyDown={(e) => e.key === "Enter" && addUser()} />
            <button onClick={addUser} className="add-user-btn"><Icon name="add" /> Добавить</button>
          </div>
        </div>
      )}
      {!isAll && value.length === 0 && <div className="permissions-warning">Добавьте логины (или отметьте роли) — пока команда доступна всем</div>}
    </div>
  );
}
