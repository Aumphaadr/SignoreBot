//! Планировщик периодических событий.
//!
//! Фаза привязана к моменту запуска движка (`epoch`), а не к моменту
//! последнего сохранения конфига: правка настроек не сдвигает сетку и не
//! вызывает немедленных срабатываний. Событие с `fire_on_start` стреляет один
//! раз при старте (если ещё не стреляло в этой сессии).

use super::{ActionCtx, Engine};
use crate::config::PeriodicEvent;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct TimerStatus {
    pub id: String,
    pub name: String,
    /// Секунд до следующего срабатывания.
    #[ts(type = "number")]
    pub next_in_sec: u64,
}

/// Следующее срабатывание после `now` для сетки `epoch + offset + k·interval`.
pub fn next_fire(epoch: Instant, now: Instant, ev: &PeriodicEvent, fired_once: bool) -> Instant {
    if ev.fire_on_start && !fired_once {
        return now;
    }
    let interval = Duration::from_secs(ev.interval_sec.max(10) as u64);
    let offset = Duration::from_secs((ev.offset_sec as u64) % interval.as_secs());
    let base = epoch + offset;
    if base > now {
        return base;
    }
    let elapsed = now.duration_since(base);
    let k = elapsed.as_secs() / interval.as_secs() + 1;
    base + interval * (k as u32)
}

pub struct Scheduler {
    epoch: Instant,
    fired: Mutex<HashSet<String>>,
    reload: Arc<Notify>,
    next: Mutex<BTreeMap<String, Instant>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self { epoch: Instant::now(), fired: Mutex::new(HashSet::new()), reload: Arc::new(Notify::new()), next: Mutex::new(BTreeMap::new()) }
    }

    /// Конфиг изменился — пересчитать расписание.
    pub fn reload(&self) {
        self.reload.notify_one();
    }

    pub fn status(&self, engine: &Engine) -> Vec<TimerStatus> {
        let cfg = engine.config.read();
        let next = self.next.lock();
        let now = Instant::now();
        cfg.periodic_events
            .iter()
            .filter(|e| e.enabled)
            .map(|e| TimerStatus {
                id: e.id.clone(),
                name: e.name.clone(),
                next_in_sec: next.get(&e.id).map(|t| t.saturating_duration_since(now).as_secs()).unwrap_or(0),
            })
            .collect()
    }

    fn plan(&self, engine: &Engine) -> Vec<(PeriodicEvent, Instant)> {
        let cfg = engine.config.read();
        let now = Instant::now();
        let fired = self.fired.lock();
        let plan: Vec<_> = cfg
            .periodic_events
            .iter()
            .filter(|e| e.enabled)
            .map(|e| (e.clone(), next_fire(self.epoch, now, e, fired.contains(&e.id))))
            .collect();
        let mut next = self.next.lock();
        next.clear();
        for (e, t) in &plan {
            next.insert(e.id.clone(), *t);
        }
        plan
    }

    /// Выполнить событие вручную (кнопка запуска в UI).
    pub async fn trigger(engine: &Engine, ev: &PeriodicEvent) {
        let author = engine.auth.info(crate::secrets::AccountKind::Bot).map(|i| i.display_name).unwrap_or_default();
        let ctx = ActionCtx { author, target: None, vars: BTreeMap::new(), label: format!("Таймер: {}", ev.name), antispam_user: None };
        engine.execute(&ev.response, &ctx).await;
    }

    /// Цикл планировщика; завершается по `cancel`.
    pub async fn run(self: Arc<Self>, engine: Arc<Engine>, cancel: CancellationToken) {
        loop {
            let plan = self.plan(&engine);
            if plan.is_empty() {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = self.reload.notified() => continue,
                }
            }
            let (ev, at) = plan.iter().min_by_key(|(_, t)| *t).cloned().unwrap();
            let wait = at.saturating_duration_since(Instant::now());
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = self.reload.notified() => continue,
                _ = tokio::time::sleep(wait) => {}
            }
            self.fired.lock().insert(ev.id.clone());
            tracing::info!(target: "signorebot::periodic", "Таймер «{}» сработал", ev.name);
            Self::trigger(&engine, &ev).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_phase_locked_and_never_immediate() {
        let epoch = Instant::now();
        let ev = PeriodicEvent { interval_sec: 60, offset_sec: 0, ..Default::default() };
        // при старте — через интервал, не сейчас
        assert_eq!(next_fire(epoch, epoch, &ev, false), epoch + Duration::from_secs(60));
        // через 61 с — следующая точка сетки
        assert_eq!(next_fire(epoch, epoch + Duration::from_secs(61), &ev, false), epoch + Duration::from_secs(120));
        // со смещением 20 с
        let ev2 = PeriodicEvent { interval_sec: 60, offset_sec: 20, ..Default::default() };
        assert_eq!(next_fire(epoch, epoch, &ev2, false), epoch + Duration::from_secs(20));
        assert_eq!(next_fire(epoch, epoch + Duration::from_secs(25), &ev2, false), epoch + Duration::from_secs(80));
        // fire_on_start — один раз
        let ev3 = PeriodicEvent { interval_sec: 60, fire_on_start: true, ..Default::default() };
        let now = epoch + Duration::from_secs(5);
        assert_eq!(next_fire(epoch, now, &ev3, false), now);
        assert_eq!(next_fire(epoch, now, &ev3, true), epoch + Duration::from_secs(60));
    }
}
