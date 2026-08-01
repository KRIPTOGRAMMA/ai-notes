// Extended activity tracking on Wayland via ext-idle-notify-v1.
//
// An application cannot (and should not) see other windows' input on Wayland,
// but the compositor can report the fact of going idle and coming back: "no
// input for N ms" (Idled) and "input appeared" (Resumed). That is enough for
// monitoring: we only need the fact of activity, not the content of the input.
// No privileges (the input group and the like) are required.
//
// The protocol gives transitions rather than a stream of events, so activity
// between Resumed and Idled is reconstructed by a ticker: until the compositor
// declares idleness we call tracker.record_input() once every TICK_SECS. The
// protocol's threshold (TIMEOUT_MS) is far smaller than the application's idle
// threshold, so the precision is sufficient.

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

// "No input for 30 s" -> Idled. The application's idle threshold
// (idle_threshold_secs, 300 s by default) is an order of magnitude larger, so
// the protocol's transitions are not an error as far as it is concerned.
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

// Tries to start extended tracking. true means the compositor supports the
// protocol and tracking works; false means we stay on the basic mode (window only).
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
        Err(_) => return false, // a compositor without ext-idle-notify-v1
    };
    let _notification = notifier.get_idle_notification(TIMEOUT_MS, &seat, &qh, ());

    let system_active = Arc::new(AtomicBool::new(true));
    let mut state = IdleState {
        system_active: system_active.clone(),
        tracker: tracker.clone(),
    };

    // The Wayland event loop: a blocking dispatch that lives for the whole run.
    std::thread::spawn(move || {
        while queue.blocking_dispatch(&mut state).is_ok() {}
    });

    // The ticker: until the compositor declares idleness the user is active
    // (possibly in another application), so we keep last_input fresh.
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(TICK_SECS));
        if system_active.load(Ordering::Relaxed) {
            tracker.record_input();
        }
    });

    true
}
