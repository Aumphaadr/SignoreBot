// Конструктор чат-сообщения из компонентов.

import Icon from "../Icon";
import { useState } from "react";
import type { ChatResponse, Component } from "../../api";
import { SAMPLE_VARS, substituteSample } from "../../api/defaults";
import Tooltip from "../Tooltip";
import "./ChatEditor.css";

function preview(components: Component[]): string {
  return components
    .map((c) => {
      switch (c.type) {
        case "space": return " ";
        case "static": return substituteSample(c.value);
        case "author": return `@${SAMPLE_VARS.user}`;
        case "target": return `@${SAMPLE_VARS.target}`;
        case "randomViewer": return "@RandomViewer";
        case "random": return String(Math.floor(Math.random() * (c.max - c.min + 1)) + c.min);
        case "phrase": {
          const valid = c.phrases.filter((p) => p.trim());
          return valid.length ? substituteSample(valid[Math.floor(Math.random() * valid.length)]) : "";
        }
        case "variable": return SAMPLE_VARS[c.name] ?? `{${c.name}}`;
      }
    })
    .join("");
}

const NEW: Record<string, () => Component> = {
  static: () => ({ type: "static", value: "" }),
  author: () => ({ type: "author" }),
  target: () => ({ type: "target" }),
  randomViewer: () => ({ type: "randomViewer" }),
  random: () => ({ type: "random", min: 1, max: 100 }),
  phrase: () => ({ type: "phrase", phrases: [""] }),
  space: () => ({ type: "space" }),
  variable: () => ({ type: "variable", name: "" }),
};

export default function ChatEditor({ value, onChange, variables = [] }: { value: ChatResponse; onChange: (v: ChatResponse) => void; variables?: string[] }) {
  const [previewKey, setPreviewKey] = useState(0);
  const comps = value.components;
  const set = (components: Component[]) => { onChange({ ...value, components }); setPreviewKey((k) => k + 1); };
  const update = (i: number, patch: Partial<Component>) => set(comps.map((c, idx) => (idx === i ? ({ ...c, ...patch } as Component) : c)));
  const remove = (i: number) => set(comps.filter((_, idx) => idx !== i));
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= comps.length) return;
    const n = [...comps];
    [n[i], n[j]] = [n[j], n[i]];
    set(n);
  };

  const removeBtn = (i: number) => (
    <button onClick={() => remove(i)} className="remove-btn" title="Удалить"><Icon name="delete"  /></button>
  );

  return (
    <div className="chat-editor">
      <div className="components-list">
        {comps.length === 0 ? (
          <div className="empty-components">
            <p><Icon name="new-item" /> Начните добавлять компоненты для создания сообщения</p>
            <p className="hint">Добавьте текст, переменные, случайные числа или наборы фраз</p>
          </div>
        ) : (
          comps.map((c, i) => (
            <div className="component-wrapper" key={i}>
              <div className="move-buttons">
                <button onClick={() => move(i, -1)} className="move-btn up" disabled={i === 0} title="Вверх"><Icon name="arrow-up"  /></button>
                <button onClick={() => move(i, 1)} className="move-btn down" disabled={i === comps.length - 1} title="Вниз"><Icon name="arrow-down"  /></button>
              </div>
              <div className="component-content">
                {c.type === "space" && (
                  <div className="component space">
                    <span className="space-icon"><Icon name="whitespace"  /> Пробел</span>
                    <Tooltip text="Вставляет пробел между соседними компонентами" />
                    {removeBtn(i)}
                  </div>
                )}
                {c.type === "static" && (
                  <div className="component static">
                    <span><Icon name="edit" /> Текст:</span>
                    <input type="text" value={c.value} onChange={(e) => update(i, { value: e.target.value })} placeholder="Введите текст... можно {user}, {target}" />
                    {removeBtn(i)}
                  </div>
                )}
                {c.type === "author" && (
                  <div className="component variable"><span><Icon name="user" /> Автор</span><Tooltip text="Имя пользователя, вызвавшего команду/событие" />{removeBtn(i)}</div>
                )}
                {c.type === "target" && (
                  <div className="component variable"><span><Icon name="target" /> Цель</span><Tooltip text="Первый аргумент после команды; если его нет — случайный зритель" />{removeBtn(i)}</div>
                )}
                {c.type === "randomViewer" && (
                  <div className="component variable"><span><Icon name="users"  /> Случайный зритель</span><Tooltip text="Подставляет случайного зрителя из чата" />{removeBtn(i)}</div>
                )}
                {c.type === "random" && (
                  <div className="component random">
                    <span><Icon name="dice" /> Случайное число:</span>
                    <input type="number" value={c.min} onChange={(e) => update(i, { min: parseInt(e.target.value) || 0 })} className="number-input" />
                    <span className="separator">—</span>
                    <input type="number" value={c.max} onChange={(e) => update(i, { max: parseInt(e.target.value) || 0 })} className="number-input" />
                    {removeBtn(i)}
                  </div>
                )}
                {c.type === "variable" && (
                  <div className="component random">
                    <span><Icon name="variable" /> Переменная:</span>
                    <select value={c.name} onChange={(e) => update(i, { name: e.target.value })} className="number-input" style={{ width: "auto" }}>
                      <option value="">—</option>
                      {variables.map((v) => <option key={v} value={v}>{`{${v}}`}</option>)}
                    </select>
                    {removeBtn(i)}
                  </div>
                )}
                {c.type === "phrase" && (
                  <div className="component phrase-set">
                    <div className="phrase-header">
                      <span><Icon name="phrase-library" /> Набор фраз</span>
                      <Tooltip text="Бот выберет случайную фразу из набора" />
                      <button onClick={() => update(i, { phrases: [...c.phrases, ""] })} className="add-phrase-btn"><Icon name="add"  /></button>
                    </div>
                    {c.phrases.map((p, pi) => (
                      <div key={pi} className="phrase-item">
                        <input type="text" value={p} onChange={(e) => update(i, { phrases: c.phrases.map((x, xi) => (xi === pi ? e.target.value : x)) })} placeholder={`Вариант ${pi + 1}`} />
                        <button onClick={() => {
                          const rest = c.phrases.filter((_, xi) => xi !== pi);
                          if (rest.length === 0) remove(i); else update(i, { phrases: rest });
                        }} className="remove-phrase-btn"><Icon name="delete"  /></button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))
        )}
      </div>

      <div className="add-component-buttons">
        <button onClick={() => set([...comps, NEW.static()])} className="add-btn"><Icon name="add"  /> Текст</button>
        <button onClick={() => set([...comps, NEW.author()])} className="add-btn"><Icon name="add"  /> Автор</button>
        <button onClick={() => set([...comps, NEW.target()])} className="add-btn"><Icon name="add"  /> Цель</button>
        <button onClick={() => set([...comps, NEW.randomViewer()])} className="add-btn"><Icon name="users"  /> Случайный зритель</button>
        <button onClick={() => set([...comps, NEW.random()])} className="add-btn"><Icon name="shuffle"  /> Случайное число</button>
        <button onClick={() => set([...comps, NEW.phrase()])} className="add-btn"><Icon name="add"  /> Набор фраз</button>
        {variables.length > 0 && <button onClick={() => set([...comps, NEW.variable()])} className="add-btn"><Icon name="add"  /> Переменная</button>}
        <button onClick={() => set([...comps, NEW.space()])} className="add-btn space-btn"><Icon name="whitespace"  /> Пробел</button>
      </div>

      <div className="preview-section">
        <div className="preview-header">
          <strong>Предпросмотр:</strong>
          <button onClick={() => setPreviewKey((k) => k + 1)} className="refresh-preview-btn" title="Обновить"><Icon name="refresh"  /> Обновить</button>
        </div>
        <div className="preview-box" key={previewKey}>
          {preview(comps) ? <span className="preview-text">{preview(comps)}</span> : <span className="preview-placeholder">Сообщение будет выглядеть так...</span>}
        </div>
      </div>
    </div>
  );
}
