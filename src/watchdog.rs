// d6 firmware sometimes freaks out when you spam keys while its drawing, and the
// key input just dies until replug. this notices and usb resets it (linux only)

#[cfg(target_os = "linux")]
mod imp {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{LazyLock, Mutex},
        time::{Duration, Instant},
    };

    use mirajazz::types::HidDeviceInfo;

    /// when to look after the last key event
    const CHECK_AFTER: [Duration; 2] = [Duration::from_millis(1500), Duration::from_millis(4000)];
    /// this many errors in the window = its broken, a couple stray ones are fine
    const STORM_LINES: usize = 20;
    const COOLDOWN: Duration = Duration::from_secs(10);

    struct Watch {
        usb_dir: PathBuf,
        generation: u64,
        last_reset: Option<Instant>,
    }

    static WATCHES: LazyLock<Mutex<HashMap<String, Watch>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub fn register(id: &str, dev: &HidDeviceInfo) {
        #[allow(irrefutable_let_patterns)]
        let async_hid::DeviceId::DevPath(path) = &dev.id else {
            return;
        };
        let Some(usb_dir) = usb_device_dir(path) else {
            log::warn!("Couldnt find usb device for {}, no watchdog", id);
            return;
        };

        WATCHES.lock().unwrap().insert(
            id.to_string(),
            Watch {
                usb_dir,
                generation: 0,
                last_reset: None,
            },
        );
    }

    /// hidraw path lives under the usb device, walk up till we hit it
    fn usb_device_dir(path: &Path) -> Option<PathBuf> {
        path.ancestors()
            .find(|dir| dir.join("busnum").exists() && dir.join("devnum").exists())
            .map(Path::to_path_buf)
    }

    pub fn note_input(id: &str) {
        let generation = {
            let mut watches = WATCHES.lock().unwrap();
            let Some(watch) = watches.get_mut(id) else {
                return;
            };
            watch.generation += 1;
            watch.generation
        };

        let id = id.to_string();
        tokio::spawn(async move {
            let started = Instant::now();
            for delay in CHECK_AFTER {
                tokio::time::sleep_until((started + delay).into()).await;

                let usb_dir = {
                    let watches = WATCHES.lock().unwrap();
                    match watches.get(&id) {
                        // newer input came in, that one checks instead
                        Some(watch) if watch.generation == generation => watch.usb_dir.clone(),
                        _ => return,
                    }
                };

                if input_is_broken(&usb_dir).await {
                    recover(&id, &usb_dir).await;
                    return;
                }
            }
        });
    }

    /// kernel spams "input irq status -75" when it happens, so just look for that
    async fn input_is_broken(usb_dir: &Path) -> bool {
        let Some(port) = usb_dir.file_name().map(|name| name.to_string_lossy().to_string()) else {
            return false;
        };

        let output = tokio::process::Command::new("journalctl")
            .args(["-k", "-q", "--no-pager", "-o", "cat", "--since", "-2s"])
            .output()
            .await;

        let Ok(output) = output else {
            return false;
        };

        let needle = format!("usb {port}: input irq status -75");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.contains(&needle))
            .count()
            >= STORM_LINES
    }

    async fn recover(id: &str, usb_dir: &Path) {
        {
            let mut watches = WATCHES.lock().unwrap();
            let Some(watch) = watches.get_mut(id) else {
                return;
            };
            if watch.last_reset.is_some_and(|last| last.elapsed() < COOLDOWN) {
                return;
            }
            watch.last_reset = Some(Instant::now());
        }

        log::warn!("Key input on {} died, resetting usb", id);

        let usb_dir = usb_dir.to_path_buf();
        let result = tokio::task::spawn_blocking(move || usb_reset(&usb_dir)).await;
        if !matches!(result, Ok(Ok(()))) {
            log::error!("Usb reset failed: {:?}", result);
            return;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
        crate::device::reinit_after_reset(id).await;
    }

    fn usb_reset(usb_dir: &Path) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;

        let read = |name: &str| -> std::io::Result<u32> {
            std::fs::read_to_string(usb_dir.join(name))?
                .trim()
                .parse()
                .map_err(|_| std::io::Error::other("bad sysfs value"))
        };
        let node = format!("/dev/bus/usb/{:03}/{:03}", read("busnum")?, read("devnum")?);
        let file = std::fs::OpenOptions::new().write(true).open(node)?;

        // USBDEVFS_RESET
        const USBDEVFS_RESET: libc::c_ulong = 0x5514;
        if unsafe { libc::ioctl(file.as_raw_fd(), USBDEVFS_RESET as _, 0) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use mirajazz::types::HidDeviceInfo;

    pub fn register(_id: &str, _dev: &HidDeviceInfo) {}
    pub fn note_input(_id: &str) {}
}

pub use imp::*;
