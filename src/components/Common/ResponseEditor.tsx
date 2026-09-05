// Вкладки «Текст в чат» / «Медиа на оверлей» (/ «Алиасы») с тумблерами.

import Icon from "../Icon";
import { useState, type ReactNode } from "react";
import type { Overlay, Response } from "../../api";
import ChatEditor from "./ChatEditor";
import MediaEditor from "./MediaEditor";
import "./ResponseEditor.css";

export default function ResponseEditor({ value, onChange, overlays, variables = [], extraTab }: {
  value: Response; onChange: (r: Response) => void; overlays: Overlay[]; variables?: string[];
  extraTab?: { label: ReactNode; content: ReactNode };
}) {
  const [tab, setTab] = useState<"chat" | "media" | "extra">("chat");
  return (
    <div className="response-editor">
      <div className="response-tabs">
        <button className={`response-tab ${tab === "chat" ? "active" : ""}`} onClick={() => setTab("chat")}>
          <span className="tab-indicator chat"></span><Icon name="chat" /> Текст в чат
          <label className="tab-toggle" onClick={(e) => e.stopPropagation()}>
            <input type="checkbox" checked={value.chat.enabled} onChange={() => onChange({ ...value, chat: { ...value.chat, enabled: !value.chat.enabled } })} />
            <span className="tab-toggle-slider"></span>
          </label>
        </button>
        <button className={`response-tab ${tab === "media" ? "active" : ""}`} onClick={() => setTab("media")}>
          <span className="tab-indicator media"></span><Icon name="clapperboard" /> Медиа на оверлей
          <label className="tab-toggle" onClick={(e) => e.stopPropagation()}>
            <input type="checkbox" checked={value.media.enabled} onChange={() => onChange({ ...value, media: { ...value.media, enabled: !value.media.enabled } })} />
            <span className="tab-toggle-slider"></span>
          </label>
        </button>
        {extraTab && <button className={`response-tab ${tab === "extra" ? "active" : ""}`} onClick={() => setTab("extra")}>{extraTab.label}</button>}
      </div>
      <div className="response-content">
        {tab === "chat" && <ChatEditor value={value.chat} onChange={(chat) => onChange({ ...value, chat })} variables={variables} />}
        {tab === "media" && <MediaEditor value={value.media} onChange={(media) => onChange({ ...value, media })} overlays={overlays} />}
        {tab === "extra" && extraTab?.content}
      </div>
    </div>
  );
}
