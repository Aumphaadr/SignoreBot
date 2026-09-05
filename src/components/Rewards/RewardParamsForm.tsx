// Параметры награды за баллы — те же, что в панели Twitch. Используется
// в окне «Новая награда на Twitch» и в блоке «Параметры награды» у наград,
// созданных через бота.

import Icon from "../Icon";
import Tooltip from "../Tooltip";
import type { ChannelReward, NewReward } from "../../api";

export const defaultParams = (): NewReward => ({
  title: "", cost: 100, prompt: "", isUserInputRequired: false, isEnabled: true, backgroundColor: "#7d77d4",
  cooldownSeconds: null, maxPerStream: null, maxPerUserPerStream: null, skipQueue: false,
});

export const paramsFromChannel = (c: ChannelReward): NewReward => ({
  title: c.title, cost: c.cost, prompt: c.prompt, isUserInputRequired: c.requiresInput, isEnabled: c.isEnabled,
  backgroundColor: c.backgroundColor || "#7d77d4", cooldownSeconds: c.cooldownSeconds, maxPerStream: c.maxPerStream,
  maxPerUserPerStream: c.maxPerUserPerStream, skipQueue: c.skipQueue,
});

/** Что мешает создать награду; null — всё в порядке. */
export function paramsProblem(p: NewReward): string | null {
  if (!p.title.trim()) return "Введите название награды";
  if (p.title.trim().length > 45) return "Название — не длиннее 45 символов";
  if (!p.cost || p.cost < 1) return "Стоимость — не меньше 1 балла";
  if (p.prompt.length > 200) return "Подсказка — не длиннее 200 символов";
  return null;
}

function LimitField({ label, hint, value, onChange, unit }: { label: string; hint: string; value: number | null; onChange: (v: number | null) => void; unit: string }) {
  return (
    <div className="form-group">
      <label>{label} <Tooltip text={hint} /></label>
      <div className="form-row" style={{ alignItems: "center", gap: 10 }}>
        <label className="toggle-label field-height" style={{ flex: "0 0 auto" }}>
          <span className="toggle-switch"><input type="checkbox" checked={value !== null} onChange={(e) => onChange(e.target.checked ? (value ?? 1) : null)} /><span className="toggle-slider"></span></span>
        </label>
        <input type="number" min={1} max={1000000} value={value ?? ""} disabled={value === null} placeholder="без ограничения" onChange={(e) => onChange(Math.max(1, parseInt(e.target.value) || 1))} style={{ flex: 1 }} />
        <span className="form-hint" style={{ margin: 0, flex: "0 0 auto" }}>{unit}</span>
      </div>
    </div>
  );
}

export default function RewardParamsForm({ value, onChange }: { value: NewReward; onChange: (p: NewReward) => void }) {
  const set = (patch: Partial<NewReward>) => onChange({ ...value, ...patch });
  return (
    <div className="reward-params-form">
      <div className="form-row">
        <div className="form-group" style={{ flex: 3 }}>
          <label>Название <Tooltip text="До 45 символов, на канале не может быть двух наград с одним названием." /></label>
          <input type="text" value={value.title} maxLength={45} onChange={(e) => set({ title: e.target.value })} placeholder="Например: Бу!" />
        </div>
        <div className="form-group" style={{ flex: 1 }}>
          <label><Icon name="channel-points" /> Стоимость, баллов</label>
          <input type="number" min={1} max={1000000} value={value.cost} onChange={(e) => set({ cost: Math.max(1, parseInt(e.target.value) || 1) })} />
        </div>
      </div>
      <div className="form-group">
        <label>Подсказка зрителю <Tooltip text="Текст под названием награды в списке у зрителя, до 200 символов. Если награда требует ввод, объясните здесь, что писать." /></label>
        <input type="text" value={value.prompt} maxLength={200} onChange={(e) => set({ prompt: e.target.value })} placeholder="Что получит зритель" />
      </div>
      <div className="form-row" style={{ alignItems: "flex-start" }}>
        <div className="form-group" style={{ flex: 1 }}>
          <label>Цвет <Tooltip text="Цвет плитки награды у зрителя." /></label>
          <div className="form-row" style={{ alignItems: "center", gap: 10 }}>
            <input type="color" value={/^#[0-9a-fA-F]{6}$/.test(value.backgroundColor) ? value.backgroundColor : "#7d77d4"} onChange={(e) => set({ backgroundColor: e.target.value })} style={{ width: 44, height: 36, padding: 2, flex: "0 0 auto" }} />
            <input type="text" value={value.backgroundColor} maxLength={7} onChange={(e) => set({ backgroundColor: e.target.value })} style={{ fontFamily: "var(--font-mono)", flex: 1 }} />
          </div>
        </div>
        <div className="form-group" style={{ flex: 1 }}>
          <label>Ввод текста <Tooltip text="Зритель должен написать что-то при активации; текст попадает в переменную {message}." /></label>
          <label className="toggle-label field-height">
            <span className="toggle-switch"><input type="checkbox" checked={value.isUserInputRequired} onChange={(e) => set({ isUserInputRequired: e.target.checked })} /><span className="toggle-slider"></span></span>
            <span className="toggle-text">{value.isUserInputRequired ? "требуется" : "не нужен"}</span>
          </label>
        </div>
        <div className="form-group" style={{ flex: 1 }}>
          <label>Награда на канале <Tooltip text="Выключенную награду зрители не видят; реакция в боте при этом сохраняется." /></label>
          <label className="toggle-label field-height">
            <span className="toggle-switch"><input type="checkbox" checked={value.isEnabled} onChange={(e) => set({ isEnabled: e.target.checked })} /><span className="toggle-slider"></span></span>
            <span className="toggle-text">{value.isEnabled ? "включена" : "выключена"}</span>
          </label>
        </div>
      </div>
      <div className="form-row" style={{ alignItems: "flex-start" }}>
        <LimitField label="Кулдаун" hint="Пауза между активациями награды для всего канала." value={value.cooldownSeconds} onChange={(v) => set({ cooldownSeconds: v })} unit="с" />
        <LimitField label="За стрим" hint="Сколько раз награду можно активировать за один стрим (всеми зрителями)." value={value.maxPerStream} onChange={(v) => set({ maxPerStream: v })} unit="раз" />
        <LimitField label="На зрителя за стрим" hint="Сколько раз один зритель может активировать награду за стрим." value={value.maxPerUserPerStream} onChange={(v) => set({ maxPerUserPerStream: v })} unit="раз" />
      </div>
      <label className="toggle-label">
        <span className="toggle-switch"><input type="checkbox" checked={value.skipQueue} onChange={(e) => set({ skipQueue: e.target.checked })} /><span className="toggle-slider"></span></span>
        <span className="toggle-text">Пропускать очередь запросов</span>
        <Tooltip text="Погашения закрываются сразу, не попадая в очередь запросов Twitch. Тогда вернуть баллы нельзя — ни боту, ни модераторам. Оставьте выключенным, если хотите возвращать баллы при недоступном оверлее." />
      </label>
      {value.skipQueue && <div className="form-hint text-warning"><Icon name="warning" /> С этим пунктом возврат баллов при недоступном оверлее работать не будет.</div>}
    </div>
  );
}
