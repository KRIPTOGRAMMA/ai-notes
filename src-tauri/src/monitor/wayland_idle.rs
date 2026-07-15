// Расширенный трекинг активности на Wayland через ext-idle-notify-v1.
//
// Приложение не может (и не должно) видеть чужой ввод на Wayland, но компоситор
// умеет сообщать факт простоя/возврата: «ввода не было N мс» (Idled) и «ввод
// появился» (Resumed). Этого достаточно для мониторинга: нам нужен только факт
// активности, не содержимое ввода. Никаких прав (группа input и т.п.) не нужно.
//
// Протокол даёт переходы, а не поток событий, поэтому активность между
// Resumed и Idled восстанавливает тикер: пока компоситор не объявил простой,
// раз в TICK_SECS дёргаем tracker.record_input(). Порог протокола (TIMEOUT_MS)
// заметно меньше порога простоя приложения, так что точность достаточная.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};

use super::activity::ActivityTracker;

// «Не было ввода 30 с» → Idled. Порог простоя приложения (idle_threshold_secs,
// дефолт 300 с) на порядок больше — переходы протокола для него не ошибка.
const TIMEOUT_MS: u32 = 30_000;
const TICK_SECS: u64 = 15;

struct IdleState {
    system_active: Arc<AtomicBool>,
    tracker: Arc<ActivityTracker>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for IdleState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for IdleState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for IdleState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for IdleState {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: <ExtIdleNotificationV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                state.system_active.store(false, Ordering::Relaxed);
            }
            ext_idle_notification_v1::Event::Resumed => {
                state.system_active.store(true, Ordering::Relaxed);
                state.tracker.record_input();
            }
            _ => {}
        }
    }
}

// Пытается запустить расширенный трекинг. true — компоситор поддерживает
// протокол и трекинг работает; false — остаёмся на базовом (только окно).
pub fn start(tracker: Arc<ActivityTracker>) -> bool {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let (globals, mut queue) = match registry_queue_init::<IdleState>(&conn) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let qh = queue.handle();

    let seat: wl_seat::WlSeat = match globals.bind(&qh, 1..=9, ()) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let notifier: ExtIdleNotifierV1 = match globals.bind(&qh, 1..=1, ()) {
        Ok(n) => n,
        Err(_) => return false, // компоситор без ext-idle-notify-v1
    };
    let _notification = notifier.get_idle_notification(TIMEOUT_MS, &seat, &qh, ());

    let system_active = Arc::new(AtomicBool::new(true));
    let mut state = IdleState {
        system_active: system_active.clone(),
        tracker: tracker.clone(),
    };

    // Поток событий Wayland: блокирующий диспатч, живёт всё время работы.
    std::thread::spawn(move || {
        while queue.blocking_dispatch(&mut state).is_ok() {}
    });

    // Тикер: пока компоситор не объявил простой — пользователь активен
    // (возможно, в другом приложении), поддерживаем last_input свежим.
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(TICK_SECS));
        if system_active.load(Ordering::Relaxed) {
            tracker.record_input();
        }
    });

    true
}
