import Icon from "../Icon";
import { copyText } from "../../api/clipboard";
import { useState } from "react";
import type { BanWord, BanWordKind } from "../../api";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
import "./BanWordsTab.css";

const MAP: Record<string, string[]> = {
  а: ["a"], б: ["6"], в: ["b"], г: ["r"], е: ["e"], к: ["k"], м: ["m"], н: ["h"], о: ["o"], р: ["p"], с: ["c"], т: ["t"], у: ["y"], х: ["x"], ь: ["b"],
  a: ["а"], b: ["в", "ь"], c: ["с"], e: ["е"], h: ["н"], k: ["к"], m: ["м"], o: ["о"], p: ["р"], t: ["т"], r: ["г"], x: ["х"], y: ["у"],
};
const MAX = 512;

/** Варианты написания (как на сервере, с тем же потолком). */
export function generateAliases(word: string): string[] {
  const orig = word.toLowerCase();
  const chars = [...orig];
  const out = new Set<string>([orig]);
  const pos = chars.map((c, i) => (MAP[c] ? { i, r: MAP[c] } : null)).filter(Boolean) as { i: number; r: string[] }[];
  const stack: [string[], number][] = [[chars, 0]];
  while (stack.length && out.size < MAX) {
    const [cur, p] = stack.pop()!;
    if (p === pos.length) { out.add(cur.join("")); continue; }
    for (const r of pos[p].r) { const n = [...cur]; n[pos[p].i] = r; stack.push([n, p + 1]); }
    stack.push([cur, p + 1]);
  }
  return [orig, ...[...out].filter((x) => x !== orig)];
}

export default function BanWordsTab() {
  const { config, setSection } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const bw = config.banwords;
  const words = bw.words;
  const [text, setText] = useState("");
  const [kind, setKind] = useState<BanWordKind>("hard");
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const setWords = (w: BanWord[]) => setSection("banwords", { ...bw, words: w });

  const add = () => {
    const w = text.trim().toLowerCase();
    if (!w) return void showNotification("Введите слово!", NOTIFICATION_TYPES.WARNING, 2000);
    if (words.some((x) => x.word === w)) return void showNotification("Такое слово уже есть в списке!", NOTIFICATION_TYPES.ERROR, 3000);
    const aliases = generateAliases(w);
    setWords([...words, { word: w, kind, aliases }]);
    setText("");
    showNotification(`Слово «${w}» добавлено, вариантов: ${aliases.length}`, NOTIFICATION_TYPES.SUCCESS, 3000);
  };
  const total = words.reduce((a, w) => a + Math.max(1, w.aliases.length), 0);

  return (
    <div className="banwords-tab">
      <div className="banwords-header">
        <h2><Icon name="ban" /> Банворды с защитой от обхода</h2>
        <p className="banwords-description">Автоматическое удаление сообщений с запрещёнными словами. Сообщение нормализуется (латиница → кириллица, повторы букв схлопываются), плюс проверяются варианты с подменой букв.</p>
      </div>
      <div className="info-stats">
        <div className="stat-card"><span className="stat-value">{words.length}</span><span className="stat-label">Слов в списке</span></div>
        <div className="stat-card"><span className="stat-value">{total}</span><span className="stat-label">Вариантов написания</span></div>
      </div>
      <div className="rules-info">
        <div className="rule-item"><span className="badge hard"><Icon name="status-disconnected" /> Жёсткий контроль</span><span>Удаляет сообщение, если слово встречается где угодно (даже как часть другого слова)</span></div>
        <div className="rule-item"><span className="badge soft"><Icon name="warning" /> Мягкий контроль</span><span>Удаляет только если слово стоит отдельно</span></div>
        <div className="rule-item"><span className="badge alias"><Icon name="moderator-shield" /> Защита от обхода</span><span>Варианты замены русских букв на похожие латинские и цифры (не более {MAX} на слово)</span></div>
      </div>
      <label className="toggle-label" style={{ marginBottom: 20 }}>
        <span className="toggle-switch"><input type="checkbox" checked={bw.skipPrivileged} onChange={(e) => setSection("banwords", { ...bw, skipPrivileged: e.target.checked })} /><span className="toggle-slider"></span></span>
        <span>Не проверять сообщения стримера и модераторов</span>
        <Tooltip text="Twitch всё равно не позволяет боту удалять их сообщения." />
      </label>
      <div className="words-list">
        {words.length === 0 ? (
          <div className="empty-words"><p><Icon name="ban" /> Список банвордов пуст</p><p className="hint">Добавьте слова, которые нужно автоматически удалять из чата</p></div>
        ) : words.map((w, i) => (
          <div key={w.word} className="word-card">
            <div className="word-card-header">
              <div className="word-info">
                <span className="word-text">{w.word}</span>
                <select value={w.kind} onChange={(e) => { setWords(words.map((x, xi) => (xi === i ? { ...x, kind: e.target.value as BanWordKind } : x))); showNotification(`Тип контроля: ${e.target.value === "hard" ? "жёсткий" : "мягкий"}`, NOTIFICATION_TYPES.INFO, 1500); }} className={`word-type-select ${w.kind}`}>
                  <option value="hard">Жёсткий</option><option value="soft">Мягкий</option>
                </select>
                <span className="aliases-count">{w.aliases.length} вариантов</span>
              </div>
              <div className="word-actions">
                <button onClick={() => setOpen((o) => ({ ...o, [w.word]: !o[w.word] }))} className="show-aliases-btn">{open[w.word] ? <Icon name="eye-off"  /> : <Icon name="eye"  />}<span>{open[w.word] ? "Скрыть" : `${w.aliases.length}`}</span></button>
                <button onClick={() => { void copyText(w.aliases.join(", ")); showNotification(`Варианты для «${w.word}» скопированы`, NOTIFICATION_TYPES.SUCCESS, 2000); }} className="copy-aliases-btn" title="Копировать варианты"><Icon name="copy"  /></button>
                <button onClick={() => { const a = generateAliases(w.word); setWords(words.map((x, xi) => (xi === i ? { ...x, aliases: a } : x))); showNotification(`Варианты для «${w.word}» обновлены (${a.length})`, NOTIFICATION_TYPES.INFO, 2000); }} className="regenerate-aliases-btn" title="Перегенерировать варианты"><Icon name="refresh" /> </button>
                <button onClick={() => showConfirm(`Удалить слово «${w.word}» из списка банвордов?`, () => { setWords(words.filter((_, xi) => xi !== i)); showNotification(`Слово «${w.word}» удалено`, NOTIFICATION_TYPES.WARNING, 2000); })} className="remove-word-btn" title="Удалить слово"><Icon name="delete"  /></button>
              </div>
            </div>
            {open[w.word] && (
              <div className="aliases-list">
                <div className="aliases-header"><strong>Варианты написания для «{w.word}»:</strong></div>
                <div className="aliases-grid">{w.aliases.map((a) => <span key={a} className="alias-item">{a}</span>)}</div>
              </div>
            )}
          </div>
        ))}
      </div>
      <div className="add-word-form">
        <h3><Icon name="add" /> Добавить слово с защитой</h3>
        <div className="form-row">
          <input type="text" value={text} onChange={(e) => setText(e.target.value)} placeholder="например, спам" onKeyDown={(e) => e.key === "Enter" && add()} />
          <select value={kind} onChange={(e) => setKind(e.target.value as BanWordKind)}><option value="hard">Жёсткий контроль</option><option value="soft">Мягкий контроль</option></select>
          <button onClick={add} className="add-word-btn"><Icon name="add"  /> Добавить</button>
        </div>
        <div className="form-hint"><Icon name="lightning" /> Автоматически сгенерируются варианты с подменой букв: а→a, е→e, б→6, р→p, с→c и другие</div>
      </div>
    </div>
  );
}
