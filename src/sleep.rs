// sleep button. screen goes off (HAN, brightness 0 barely dims it), next key press turns it back on
// and still does its thing, unless that key is the sleep button itself

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use mirajazz::state::DeviceStateUpdate;
use openaction::KeyEvent;

use crate::{DEVICES, mappings::COL_COUNT};

pub const UUID: &str = "st.lynx.plugins.opendeck-ampgd6.sleep";

#[derive(Default)]
struct State {
    asleep: bool,
    /// key that woke it up, so the sleep button doesnt put it right back to sleep
    woken_by: Option<u8>,
}

static STATES: LazyLock<Mutex<HashMap<String, State>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub async fn key_down(event: KeyEvent) {
    if event.action != UUID {
        return;
    }

    {
        let mut states = STATES.lock().unwrap();
        let state = states.entry(event.device.clone()).or_default();

        let coords = event.payload.coordinates;
        let key = coords.row * COL_COUNT as u8 + coords.column;
        if state.woken_by.take() == Some(key) {
            // this press was just waking it up
            return;
        }
        state.asleep = true;
    }

    log::info!("Putting {} to sleep", event.device);
    if let Some(device) = DEVICES.read().await.get(&event.device) {
        if let Err(err) = device.sleep().await {
            log::error!("Failed to sleep {}: {}", event.device, err);
        }
    }
}

/// device reconnected, its awake again
pub fn forget(id: &str) {
    STATES.lock().unwrap().remove(id);
}

pub fn is_asleep(id: &str) -> bool {
    STATES.lock().unwrap().get(id).is_some_and(|state| state.asleep)
}

/// call on every input before it goes to opendeck, wakes the screen if its off
pub async fn note_input(id: &str, update: &DeviceStateUpdate) {
    let DeviceStateUpdate::ButtonDown(key) = update else {
        return;
    };

    {
        let mut states = STATES.lock().unwrap();
        let Some(state) = states.get_mut(id) else {
            return;
        };

        if !state.asleep {
            state.woken_by = None;
            return;
        }
        state.asleep = false;
        state.woken_by = Some(*key);
    }

    log::info!("Waking {} up", id);
    crate::device::reinit_after_reset(id).await;
}
