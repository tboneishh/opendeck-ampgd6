use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, LazyLock, Mutex, atomic::AtomicU8},
    time::{Duration, Instant},
};

use data_url::DataUrl;
use image::{
    DynamicImage, RgbImage, codecs::jpeg::JpegEncoder, imageops::FilterType,
    load_from_memory_with_format,
};
use mirajazz::{
    device::Device, error::MirajazzError, state::DeviceStateUpdate, types::ImageRotation,
};
use openaction::{OUTBOUND_EVENT_MANAGER, SetImageEvent};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{
    DEVICES, TOKENS,
    inputs::opendeck_to_device,
    mappings::{
        COL_COUNT, CandidateDevice, ENCODER_COUNT, IMAGE_OFFSET_X, IMAGE_OFFSET_Y, KEY_COUNT,
        Kind, ROW_COUNT, get_image_format_for_key,
    },
};

/// Initializes a device and listens for events
pub async fn device_task(candidate: CandidateDevice, token: CancellationToken) {
    log::info!("Running device task for {:?}", candidate);

    // Wrap in a closure so we can use `?` operator
    let device = async || -> Result<Device, MirajazzError> {
        log::info!("Connecting to device...");
        let device = connect(&candidate).await?;
        log::info!("Device connected successfully");

        // Try to set brightness - some devices may not support this command
        log::info!("Setting brightness...");
        if let Err(e) = device.set_brightness(50).await {
            log::warn!(
                "Failed to set brightness (this may be normal for this device): {}",
                e
            );
            // Continue anyway - brightness setting might not be supported
        } else {
            log::info!("Brightness set successfully");
        }

        // Try to clear all button images - some devices may not support this command
        log::info!("Clearing all button images...");
        if let Err(e) = device.clear_all_button_images().await {
            log::warn!(
                "Failed to clear all button images (this may be normal for this device): {}",
                e
            );
            // Continue anyway - clearing might not be supported or needed
        } else {
            log::info!("Button images cleared successfully");
        }

        // Try to flush - some devices may not need this
        log::info!("Flushing device...");
        if let Err(e) = device.flush().await {
            log::warn!(
                "Failed to flush device (this may be normal for this device): {}",
                e
            );
            // Continue anyway
        } else {
            log::info!("Device flushed successfully");
        }

        Ok(device)
    }()
    .await;

    let device: Device = match device {
        Ok(device) => device,
        Err(err) => {
            handle_error(&candidate.id, err).await;

            log::error!(
                "Had error during device init, finishing device task: {:?}",
                candidate
            );

            return;
        }
    };

    log::info!("Registering device {}", candidate.id);
    if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
        outbound
            .register_device(
                candidate.id.clone(),
                candidate.kind.human_name(),
                ROW_COUNT as u8,
                COL_COUNT as u8,
                ENCODER_COUNT as u8,
                0,
            )
            .await
            .unwrap();
    }

    reset_render_state(&candidate.id);
    crate::sleep::forget(&candidate.id);
    DEVICES.write().await.insert(candidate.id.clone(), device);
    crate::watchdog::register(&candidate.id, &candidate.dev);

    tokio::select! {
        _ = device_events_task(&candidate) => {},
        _ = token.cancelled() => {}
    };

    log::info!("Shutting down device {:?}", candidate);

    if let Some(device) = DEVICES.read().await.get(&candidate.id) {
        device.shutdown().await.ok();
    }

    log::info!("Device task finished for {:?}", candidate);
}

/// Handles errors, returning true if should continue, returning false if an error is fatal
pub async fn handle_error(id: &String, err: MirajazzError) -> bool {
    log::error!("Device {} error: {}", id, err);

    // Some errors are not critical and can be ignored without sending disconnected event
    if matches!(err, MirajazzError::ImageError(_) | MirajazzError::BadData) {
        return true;
    }

    log::info!("Deregistering device {}", id);
    if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
        outbound.deregister_device(id.clone()).await.unwrap();
    }

    log::info!("Cancelling tasks for device {}", id);
    if let Some(token) = TOKENS.read().await.get(id) {
        token.cancel();
    }

    log::info!("Removing device {} from the list", id);
    DEVICES.write().await.remove(id);

    log::info!("Finished clean-up for {}", id);

    false
}

pub async fn connect(candidate: &CandidateDevice) -> Result<Device, MirajazzError> {
    let result = Device::connect(
        &candidate.dev,
        candidate.kind.protocol_version(),
        KEY_COUNT,
        ENCODER_COUNT,
    )
    .await;

    match result {
        Ok(device) => Ok(device),
        Err(e) => {
            log::error!("Error while connecting to device: {e}");

            Err(e)
        }
    }
}

/// Handles events from device to OpenDeck
async fn device_events_task(candidate: &CandidateDevice) -> Result<(), MirajazzError> {
    log::info!("Connecting to {} for incoming events", candidate.id);

    let devices_lock = DEVICES.read().await;
    let mut reader = match devices_lock.get(&candidate.id) {
        Some(device) => device.get_reader(crate::inputs::process_input),
        None => return Ok(()),
    };
    drop(devices_lock);

    // Force event reader to use protocol 3 while keeping device write protocol unchanged.
    if let Some(reader_mut) = std::sync::Arc::get_mut(&mut reader) {
        reader_mut.protocol_version = 3;
    }

    log::info!("Connected to {} for incoming events", candidate.id);

    log::info!("Reader is ready for {}", candidate.id);

    // Track last processed event to avoid duplicates
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    #[derive(Hash, PartialEq, Eq, Clone, Copy)]
    enum EventKey {
        ButtonDown(u8),
        ButtonUp(u8),
        EncoderDown(u8),
        EncoderUp(u8),
        EncoderTwist(u8, i16),
    }

    let mut last_events: HashSet<(EventKey, Instant)> = HashSet::new();
    let dedup_window = Duration::from_millis(500); // 500ms window for deduplication

    loop {
        log::info!("Reading updates...");

        let updates = match reader.read(None).await {
            Ok(updates) => updates,
            Err(e) => {
                if !handle_error(&candidate.id, e).await {
                    break;
                }

                continue;
            }
        };

        // Clean up old events from deduplication cache
        let now = Instant::now();
        last_events.retain(|(_, time)| now.duration_since(*time) < dedup_window);

        if !updates.is_empty() {
            crate::watchdog::note_input(&candidate.id);
        }

        for update in updates {
            log::info!("New update: {:#?}", update);

            // Create a key for deduplication
            let event_key = match &update {
                DeviceStateUpdate::ButtonDown(key) => EventKey::ButtonDown(*key),
                DeviceStateUpdate::ButtonUp(key) => EventKey::ButtonUp(*key),
                DeviceStateUpdate::EncoderDown(enc) => EventKey::EncoderDown(*enc),
                DeviceStateUpdate::EncoderUp(enc) => EventKey::EncoderUp(*enc),
                DeviceStateUpdate::EncoderTwist(enc, val) => {
                    EventKey::EncoderTwist(*enc, *val as i16)
                }
            };

            // Check for duplicates (same event type and key/encoder within the dedup window)
            let is_duplicate = last_events.iter().any(|(key, _)| *key == event_key);

            if is_duplicate {
                log::debug!("Skipping duplicate event: {:#?}", update);
                continue;
            }

            // a press forgets the last release for that key (and the other way round),
            // otherwise a quick re-press within the window loses its key up and the button sticks
            let opposite = match event_key {
                EventKey::ButtonDown(key) => Some(EventKey::ButtonUp(key)),
                EventKey::ButtonUp(key) => Some(EventKey::ButtonDown(key)),
                EventKey::EncoderDown(enc) => Some(EventKey::EncoderUp(enc)),
                EventKey::EncoderUp(enc) => Some(EventKey::EncoderDown(enc)),
                EventKey::EncoderTwist(..) => None,
            };
            if let Some(opposite) = opposite {
                last_events.retain(|(key, _)| *key != opposite);
            }

            // Add to deduplication cache
            last_events.insert((event_key, now));

            // wake up first if the sleep button turned the screen off
            crate::sleep::note_input(&candidate.id, &update).await;

            let id = candidate.id.clone();

            if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
                match update {
                    DeviceStateUpdate::ButtonDown(key) => {
                        log::info!("Sending key_down event: device_id={}, key={}", id, key);
                        outbound.key_down(id.clone(), key).await.unwrap();
                    }
                    DeviceStateUpdate::ButtonUp(key) => {
                        log::info!("Sending key_up event: device_id={}, key={}", id, key);
                        outbound.key_up(id.clone(), key).await.unwrap();
                    }
                    DeviceStateUpdate::EncoderDown(encoder) => {
                        outbound.encoder_down(id, encoder).await.unwrap();
                    }
                    DeviceStateUpdate::EncoderUp(encoder) => {
                        outbound.encoder_up(id, encoder).await.unwrap();
                    }
                    DeviceStateUpdate::EncoderTwist(encoder, val) => {
                        outbound
                            .encoder_change(id, encoder, val as i16)
                            .await
                            .unwrap();
                    }
                }
            }
        }
    }

    Ok(())
}

// panel is slow af per key and only updates on STP, so send stuff asap and STP once at the end

/// how long before leftover keys get blanked
const CLEAR_ALL_GRACE: Duration = Duration::from_millis(300);
/// wait this long for more stuff before STP
const COMMIT_QUIET: Duration = Duration::from_millis(60);
/// dont wait longer than this tho
const COMMIT_MAX: Duration = Duration::from_millis(400);
/// looks the same, smaller
const JPEG_QUALITY: u8 = 80;
/// packet size
const IMAGE_REPORT_SIZE: usize = 512;

#[derive(Clone)]
enum KeyOp {
    Image { data: Arc<Vec<u8>>, hash: u64 },
    Clear,
}

#[derive(Default)]
struct RenderState {
    /// stuff waiting to go out
    pending: [Option<KeyOp>; KEY_COUNT],
    /// keys that get blanked if nothing shows up for them
    clear_all_keys: [bool; KEY_COUNT],
    clear_all_at: Option<Instant>,
    /// whats on screen rn
    shown: [Option<(u64, Arc<Vec<u8>>)>; KEY_COUNT],
    last_queued: Option<Instant>,
    /// whole page is in, just send it
    page_complete: bool,
    worker_running: bool,
    wake: Arc<Notify>,
}

static RENDER_STATES: LazyLock<Mutex<HashMap<String, RenderState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// device got wiped, forget everything
pub fn reset_render_state(id: &str) {
    RENDER_STATES.lock().unwrap().remove(id);
}

/// queue stuff for a key (or all of them)
fn queue_op(id: &str, position: Option<u8>, op: KeyOp) {
    let mut states = RENDER_STATES.lock().unwrap();
    let state = states.entry(id.to_string()).or_default();
    state.last_queued = Some(Instant::now());

    match position {
        Some(position) if (position as usize) < KEY_COUNT => {
            state.pending[position as usize] = Some(op);
            state.clear_all_keys[position as usize] = false;
        }
        Some(position) => {
            log::warn!("Ignoring image for out of range key {}", position);
            return;
        }
        None => {
            // dont blank yet, new stuff is coming
            state.pending = Default::default();
            state.clear_all_keys = [true; KEY_COUNT];
            state.clear_all_at = Some(Instant::now() + CLEAR_ALL_GRACE);
            state.page_complete = false;
        }
    }

    kick_worker(id, state);
}

fn kick_worker(id: &str, state: &mut RenderState) {
    if state.worker_running {
        state.wake.notify_one();
    } else {
        state.worker_running = true;
        tokio::spawn(render_worker(id.to_string(), state.wake.clone()));
    }
}

/// last brightness opendeck asked for, needed after a usb reset
pub static BRIGHTNESS: AtomicU8 = AtomicU8::new(50);

/// device got usb reset or woke from sleep, turn the screen on and redraw everything
pub async fn reinit_after_reset(id: &str) {
    if let Some(device) = DEVICES.read().await.get(id) {
        // reset while the sleep button is on, keep it dark. waking redraws anyway
        if crate::sleep::is_asleep(id) {
            device.sleep().await.ok();
            return;
        }

        let mut dis = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x44, 0x49, 0x53];
        device.write_extended_data(&mut dis).await.ok();
        device.set_brightness(BRIGHTNESS.load(std::sync::atomic::Ordering::Relaxed)).await.ok();
    }

    let mut states = RENDER_STATES.lock().unwrap();
    let Some(state) = states.get_mut(id) else {
        return;
    };

    // no clue whats on screen, resend all of it (stuff that changed while asleep wins)
    for key in 0..KEY_COUNT {
        let shown = state.shown[key].take();
        if state.pending[key].is_none() {
            state.pending[key] = Some(match shown {
                Some((hash, data)) => KeyOp::Image { data, hash },
                None => KeyOp::Clear,
            });
        }
        state.shown[key] = Some((0, Arc::new(vec![])));
    }
    state.last_queued = Some(Instant::now());
    kick_worker(id, state);
}

enum Next {
    Send(u8, KeyOp),
    WaitUntil(Instant),
    Idle(Instant),
}

/// figures out whats next, none = done
fn next_step(id: &str, dirty: bool) -> Option<Next> {
    let mut states = RENDER_STATES.lock().unwrap();
    let state = states.get_mut(id)?;

    // drawing would turn the screen back on, keep it queued till wake
    if crate::sleep::is_asleep(id) {
        state.worker_running = false;
        return None;
    }

    if let Some(deadline) = state.clear_all_at {
        if Instant::now() >= deadline {
            for key in 0..KEY_COUNT {
                if std::mem::take(&mut state.clear_all_keys[key]) && state.pending[key].is_none() {
                    state.pending[key] = Some(KeyOp::Clear);
                }
            }
            state.clear_all_at = None;
        }
    }

    // got everything for the page?
    if state.clear_all_at.is_some() && state.clear_all_keys.iter().all(|flagged| !flagged) {
        state.clear_all_at = None;
        state.page_complete = true;
    }

    // skip stuff thats already there
    for key in 0..KEY_COUNT {
        let noop = match (&state.pending[key], &state.shown[key]) {
            (Some(KeyOp::Image { hash, .. }), Some((shown, _))) => hash == shown,
            (Some(KeyOp::Clear), None) => true,
            _ => false,
        };
        if noop {
            state.pending[key] = None;
        }
    }

    // images first
    let key = (0..KEY_COUNT)
        .find(|&key| matches!(state.pending[key], Some(KeyOp::Image { .. })))
        .or_else(|| (0..KEY_COUNT).find(|&key| state.pending[key].is_some()));

    if let Some(key) = key {
        let op = state.pending[key].take().unwrap();
        state.shown[key] = match &op {
            KeyOp::Image { data, hash } => Some((*hash, data.clone())),
            KeyOp::Clear => None,
        };

        return Some(Next::Send(key as u8, op));
    }

    if dirty {
        if std::mem::take(&mut state.page_complete) {
            return Some(Next::Idle(Instant::now()));
        }
        let last = state.last_queued.unwrap_or_else(Instant::now);
        return Some(Next::Idle(last + COMMIT_QUIET));
    }

    if let Some(deadline) = state.clear_all_at {
        return Some(Next::WaitUntil(deadline));
    }

    state.page_complete = false;
    state.worker_running = false;
    None
}

/// sends everything, STP at the end
async fn render_worker(id: String, wake: Arc<Notify>) {
    let mut dirty = false;
    let mut burst: Option<(Instant, usize)> = None;

    while let Some(next) = next_step(&id, dirty) {
        let result = match next {
            Next::Send(key, op) => {
                let started = burst.get_or_insert((Instant::now(), 0));
                started.1 += 1;
                dirty = true;

                let result = match DEVICES.read().await.get(&id) {
                    Some(device) => send_op(device, opendeck_to_device(key), &op).await,
                    None => break,
                };
                // tiny breather, the d6 drops key input if you hammer it
                tokio::time::sleep(Duration::from_millis(1)).await;
                result
            }
            Next::WaitUntil(deadline) => {
                let _ = tokio::time::timeout_at(deadline.into(), wake.notified()).await;
                Ok(())
            }
            Next::Idle(quiet_at) => {
                let cap = burst.map_or(quiet_at, |(started, _)| started + COMMIT_MAX);
                let deadline = quiet_at.min(cap);

                if Instant::now() < deadline {
                    // wait for the rest
                    let _ = tokio::time::timeout_at(deadline.into(), wake.notified()).await;
                    continue;
                }

                let result = commit(&id).await;
                dirty = false;
                log_burst(&mut burst);
                result
            }
        };

        if let Err(err) = result {
            // something broke, start over
            reset_render_state(&id);
            handle_error(&id, err).await;
            return;
        }
    }
}

fn log_burst(burst: &mut Option<(Instant, usize)>) {
    if let Some((started, count)) = burst.take() {
        log::info!("Drew {} op(s) in {:?}", count, started.elapsed());
    }
}

/// blank key. CLE skips STP and looks janky so just send black
static BLACK_FRAME: LazyLock<Vec<u8>> = LazyLock::new(|| {
    encode_image(DynamicImage::ImageRgb8(RgbImage::new(144, 144))).unwrap_or_default()
});

/// upload one key
async fn send_op(device: &Device, key: u8, op: &KeyOp) -> Result<(), MirajazzError> {
    let data: &[u8] = match op {
        KeyOp::Clear => &BLACK_FRAME,
        KeyOp::Image { data, .. } => data,
    };

    // same as mirajazz but sends right away
    let mut header = vec![
        0x00,
        0x43,
        0x52,
        0x54,
        0x00,
        0x00,
        0x42,
        0x41,
        0x54,
        0x00,
        0x00,
        (data.len() >> 8) as u8,
        data.len() as u8,
        key + 1,
    ];
    device.write_extended_data(&mut header).await?;

    let mut report = Vec::with_capacity(IMAGE_REPORT_SIZE + 1);
    for chunk in data.chunks(IMAGE_REPORT_SIZE) {
        report.clear();
        report.push(0x00);
        report.extend_from_slice(chunk);
        report.resize(IMAGE_REPORT_SIZE + 1, 0);

        device.write_data(&report).await?;
    }

    Ok(())
}

/// show it
async fn commit(id: &str) -> Result<(), MirajazzError> {
    let devices = DEVICES.read().await;
    let Some(device) = devices.get(id) else {
        return Ok(());
    };

    let mut stop = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x53, 0x54, 0x50];
    device.write_extended_data(&mut stop).await
}

/// resize, flip, jpeg
fn encode_image(image: DynamicImage) -> Result<Vec<u8>, MirajazzError> {
    let format = get_image_format_for_key(&Kind::AMPGD6, 0);
    let (width, height) = (format.size.0 as u32, format.size.1 as u32);

    let mut scaled = image
        .resize_exact(width, height, FilterType::Lanczos3)
        .into_rgb8();

    if IMAGE_OFFSET_X != 0 || IMAGE_OFFSET_Y != 0 {
        let mut shifted = RgbImage::new(width, height);
        image::imageops::overlay(&mut shifted, &scaled, IMAGE_OFFSET_X, IMAGE_OFFSET_Y);
        scaled = shifted;
    }

    let rotated = match format.rotation {
        ImageRotation::Rot0 => scaled,
        ImageRotation::Rot90 => image::imageops::rotate90(&scaled),
        ImageRotation::Rot180 => image::imageops::rotate180(&scaled),
        ImageRotation::Rot270 => image::imageops::rotate270(&scaled),
    };

    let mut data = Vec::new();
    JpegEncoder::new_with_quality(&mut data, JPEG_QUALITY)
        .encode_image(&rotated)
        .map_err(MirajazzError::ImageError)?;

    Ok(data)
}

/// Handles different combinations of "set image" event, including clearing the specific buttons and whole device
pub async fn handle_set_image(evt: SetImageEvent) -> Result<(), MirajazzError> {
    match (evt.position, evt.image) {
        (Some(position), Some(image)) => {
            log::debug!("Queueing image for button {}", position);

            // OpenDeck sends image as a data url, so parse it using a library
            let Ok(url) = DataUrl::process(image.as_str()) else {
                log::error!("Received image that isn't a data url");
                return Ok(());
            };
            let Ok((body, _fragment)) = url.decode_to_vec() else {
                log::error!("Failed to decode image data url");
                return Ok(());
            };

            // Allow only image/jpeg mime for now
            if url.mime_type().subtype != "jpeg" {
                log::error!("Incorrect mime type: {}", url.mime_type());

                return Ok(()); // Not a fatal error, enough to just log it
            }

            let image = load_from_memory_with_format(body.as_slice(), image::ImageFormat::Jpeg)?;
            let data = tokio::task::spawn_blocking(move || encode_image(image))
                .await
                .map_err(|_| MirajazzError::BadData)??;

            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);

            queue_op(
                &evt.device,
                Some(position),
                KeyOp::Image {
                    data: Arc::new(data),
                    hash: hasher.finish(),
                },
            );
        }
        (Some(position), None) => queue_op(&evt.device, Some(position), KeyOp::Clear),
        (None, None) => queue_op(&evt.device, None, KeyOp::Clear),
        _ => {}
    }

    Ok(())
}
