// Выбор шрифта: каждый пункт списка показан своим шрифтом. Нативный <select>
// так не умеет (WebKit игнорирует font-family у <option>), поэтому свой список.

import { useEffect, useRef, useState } from "react";
import Icon from "../Icon";
import { FONT_FAMILIES } from "../../api/fonts.generated";
import "./FontPicker.css";

export default function FontPicker({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", close); document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", close); document.removeEventListener("keydown", esc); };
  }, [open]);
  const current = FONT_FAMILIES.find((f) => f.value === value);
  const label = current?.label ?? value.replace(/^["']|["'],.*$/g, "").replace(/^["']|["']$/g, "");
  return (
    <div className={`font-picker ${open ? "open" : ""}`} ref={ref}>
      <button type="button" className="font-picker-current" onClick={() => setOpen((o) => !o)} aria-haspopup="listbox" aria-expanded={open}>
        <span style={{ fontFamily: value }}>{label}</span>
        <Icon name="arrow-down" className="font-picker-arrow" />
      </button>
      {open && (
        <ul className="font-picker-list" role="listbox">
          {!current && <li className="font-picker-item muted" role="option" aria-selected="true" style={{ fontFamily: value }} onClick={() => setOpen(false)}>{label} <small>(своё значение)</small></li>}
          {FONT_FAMILIES.map((f) => (
            <li key={f.value} role="option" aria-selected={f.value === value} className={`font-picker-item ${f.value === value ? "active" : ""}`} style={{ fontFamily: f.value }} onClick={() => { onChange(f.value); setOpen(false); }}>
              <span className="font-picker-name">{f.label}</span>
              <span className="font-picker-sample">Съешь ещё этих мягких булок 0123</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
