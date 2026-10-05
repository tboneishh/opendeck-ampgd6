// page buttons. pages = profiles named 1, 2, 3 etc

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

use base64::prelude::*;
use openaction::{AppearEvent, KeyEvent, OutboundEventManager};

pub const NEXT_UUID: &str = "st.lynx.plugins.opendeck-ampgd6.nextpage";
pub const PREV_UUID: &str = "st.lynx.plugins.opendeck-ampgd6.prevpage";
pub const COUNTER_UUID: &str = "st.lynx.plugins.opendeck-ampgd6.pagecounter";

pub async fn key_down(event: KeyEvent, outbound: &mut OutboundEventManager) {
    let pages = list_pages(&event.device);
    if pages.is_empty() {
        log::warn!("No profiles found for {}", event.device);
        return;
    }

    let current = current_page(&event.context, &event.device, &pages);
    let target = match event.action.as_str() {
        NEXT_UUID => (current + 1) % pages.len(),
        PREV_UUID => (current + pages.len() - 1) % pages.len(),
        COUNTER_UUID => 0,
        _ => return,
    };

    if target == current {
        return;
    }

    log::info!("Switching {} to page {} ({})", event.device, target + 1, pages[target]);

    if let Err(err) = switch_profile(&event.device, &pages[target]).await {
        log::error!("Failed to switch profile: {}", err);
        outbound.show_alert(event.context).await.ok();
    }
}

pub async fn will_appear(event: AppearEvent, outbound: &mut OutboundEventManager) {
    if event.action != COUNTER_UUID {
        return;
    }

    let pages = list_pages(&event.device);
    let (current, total) = if pages.is_empty() {
        ("?".to_string(), "?".to_string())
    } else {
        let current = current_page(&event.context, &event.device, &pages);
        ((current + 1).to_string(), pages.len().to_string())
    };

    // opendeck text looks bad so draw our own
    let image = format!(
        "data:image/svg+xml;base64,{}",
        BASE64_STANDARD.encode(counter_svg(&current, &total))
    );
    outbound
        .set_title(event.context.clone(), Some(String::new()), None)
        .await
        .ok();
    outbound.set_image(event.context, Some(image), None).await.ok();
}

fn counter_svg(current: &str, total: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 144 144" width="144" height="144">
<rect width="144" height="144" fill="#1c1c1e"/>
<text x="72" y="84" text-anchor="middle" font-family="Inter, Cantarell, 'Noto Sans', 'Segoe UI', 'Helvetica Neue', Arial, sans-serif" font-weight="700" font-size="62" fill="#ffffff">{current}</text>
<text x="72" y="114" text-anchor="middle" font-family="Inter, Cantarell, 'Noto Sans', 'Segoe UI', 'Helvetica Neue', Arial, sans-serif" font-weight="500" font-size="20" fill="#8e8e93">of {total}</text>
</svg>"##
    )
}

/// opendeck config folder
fn config_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.parent()?.parent()?.to_path_buf())
}

fn list_pages(device: &str) -> Vec<String> {
    let Some(dir) = config_dir().map(|dir| dir.join("profiles").join(device)) else {
        return vec![];
    };

    let mut pages = vec![];
    collect_profiles(&dir, &dir, &mut pages);
    order_pages(pages)
}

fn order_pages(mut pages: Vec<String>) -> Vec<String> {
    let is_number = |name: &String| !name.is_empty() && name.chars().all(|c| c.is_ascii_digit());
    if pages.iter().any(is_number) {
        pages.retain(is_number);
    }

    pages.sort_by(|a, b| match (a == "Default", b == "Default") {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => natural_cmp(a, b),
    });
    pages
}

/// finds all the profiles
fn collect_profiles(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_profiles(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(rel) = path.with_extension("").strip_prefix(root) {
                let name = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(name);
            }
        }
    }
}

/// which page were on
fn current_page(context: &str, device: &str, pages: &[String]) -> usize {
    let from_context = context.split('.').nth(1).map(str::to_string);
    let profile = from_context.or_else(|| selected_profile(device));

    profile
        .and_then(|profile| pages.iter().position(|page| *page == profile))
        .unwrap_or(0)
}

fn selected_profile(device: &str) -> Option<String> {
    let path = config_dir()?.join("profiles").join(format!("{device}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("selected_profile")?.as_str().map(str::to_string)
}

/// so 2 comes before 10
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());

    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take_num = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits.trim_start_matches('0').to_string()
                };
                let (na, nb) = (take_num(&mut a), take_num(&mut b));
                let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// opendeck wont let us switch normally so this is the workaround. dbus is way faster
async fn switch_profile(device: &str, profile: &str) -> Result<(), String> {
    let message = serde_json::json!({
        "event": "switchProfile",
        "device": device,
        "profile": profile,
    })
    .to_string();

    #[cfg(target_os = "linux")]
    {
        let argv = format!(
            "['opendeck', '--process-message', {}]",
            gvariant_string(&message)
        );
        let status = tokio::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "me.amankhanna.opendeck.SingleInstance",
                "--object-path",
                "/me/amankhanna/opendeck/SingleInstance",
                "--method",
                "org.SingleInstance.DBus.ExecuteCallback",
                &argv,
                "'/'",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        if matches!(status, Ok(status) if status.success()) {
            return Ok(());
        }
        log::warn!("gdbus call failed, falling back to opendeck cli");
    }

    for exe in opendeck_candidates() {
        let status = tokio::process::Command::new(&exe)
            .args(["--process-message", &message])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        if matches!(status, Ok(status) if status.success()) {
            return Ok(());
        }
    }

    Err("couldnt reach opendeck".to_string())
}

/// quoting for gdbus
#[cfg(target_os = "linux")]
fn gvariant_string(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// places opendeck might be
fn opendeck_candidates() -> Vec<PathBuf> {
    let mut out = vec![];

    #[cfg(target_os = "linux")]
    if let Ok(exe) = std::fs::read_link(format!("/proc/{}/exe", std::os::unix::process::parent_id())) {
        out.push(exe);
    }

    #[cfg(target_os = "macos")]
    {
        let ppid = std::os::unix::process::parent_id().to_string();
        if let Ok(output) = std::process::Command::new("ps").args(["-o", "comm=", "-p", &ppid]).output() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                out.push(PathBuf::from(path));
            }
        }
        out.push("/Applications/OpenDeck.app/Contents/MacOS/opendeck".into());
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            out.push(Path::new(&local).join("OpenDeck").join("opendeck.exe"));
        }
        out.push(r"C:\Program Files\OpenDeck\opendeck.exe".into());
    }

    out.push("opendeck".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_pages_naturally() {
        let mut pages = vec!["p10", "p2", "Default", "p1", "abc"];
        pages.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(pages, vec!["abc", "Default", "p1", "p2", "p10"]);
    }

    #[test]
    fn numbered_profiles_are_pages() {
        let names = ["test", "2", "Default", "10", "1", "p2"].map(String::from).to_vec();
        assert_eq!(order_pages(names), vec!["1", "2", "10"]);

        let names = ["p10", "p2", "Default"].map(String::from).to_vec();
        assert_eq!(order_pages(names), vec!["Default", "p2", "p10"]);
    }

    #[test]
    fn reads_profile_from_context() {
        let pages = vec!["Default".to_string(), "p2".to_string()];
        assert_eq!(current_page("d6-1-AMPGD6.p2.Keypad.4.0", "x", &pages), 1);
    }
}
