use std::process::Command;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use crate::engine::Provider;
use crate::model::{Action, Item};

/// Network info: local IP, public IP (explicit only), listening ports, and
/// preferred Wi-Fi networks. Everything is keyword-gated. The public-IP lookup
/// is a single HTTP GET (no polling), bounded by a hard timeout and cached
/// briefly so it never hangs the worker thread.
pub struct NetworkProvider {
    public_ip: Mutex<Option<(Instant, String)>>,
}

impl NetworkProvider {
    pub fn new() -> Self {
        Self {
            public_ip: Mutex::new(None),
        }
    }
}

const PUBLIC_IP_TTL: Duration = Duration::from_secs(300);

impl Provider for NetworkProvider {
    fn name(&self) -> &'static str {
        "network"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let lower = query.trim().to_ascii_lowercase();
        if lower.is_empty() {
            return;
        }

        match lower.as_str() {
            "ip" | "localip" | "local ip" | "my ip" | "ipaddr" => {
                self.push_local_ip(out);
            }
            "myip" | "publicip" | "public ip" | "external ip" | "wan ip" => {
                self.push_public_ip(out);
                self.push_local_ip(out);
            }
            "ports" | "listening" | "listening ports" | "open ports" => {
                push_listening_ports(out);
            }
            "wifi networks" | "wifi list" | "networks" | "wifi" => {
                push_wifi_networks(out);
            }
            other => {
                if let Some(rest) = other.strip_prefix("port ") {
                    if let Ok(port) = rest.trim().parse::<u16>() {
                        push_port_lookup(out, port);
                    }
                }
            }
        }
    }
}

impl NetworkProvider {
    fn push_local_ip(&self, out: &mut Vec<Item>) {
        if let Some(ip) = local_ip() {
            out.push(Item::new(
                ip.clone(),
                "Local IP address - Enter to copy",
                "Network",
                9_100,
                Action::CopyText(ip),
            ));
        } else {
            out.push(Item::new(
                "No local IP found",
                "Not connected to a network?",
                "Network",
                9_100,
                Action::None,
            ));
        }
    }

    fn push_public_ip(&self, out: &mut Vec<Item>) {
        // Serve a fresh-enough cached value if available.
        {
            let guard = self.public_ip.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((t, ip)) = guard.as_ref() {
                if t.elapsed() < PUBLIC_IP_TTL {
                    out.push(public_ip_item(ip));
                    return;
                }
            }
        }
        match fetch_public_ip() {
            Some(ip) => {
                *self.public_ip.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((Instant::now(), ip.clone()));
                out.push(public_ip_item(&ip));
            }
            None => out.push(Item::new(
                "Public IP unavailable",
                "Request timed out or you're offline",
                "Network",
                9_200,
                Action::None,
            )),
        }
    }
}

fn public_ip_item(ip: &str) -> Item {
    Item::new(
        ip.to_string(),
        "Public IP address - Enter to copy",
        "Network",
        9_200,
        Action::CopyText(ip.to_string()),
    )
}

fn local_ip() -> Option<String> {
    for iface in ["en0", "en1", "en2"] {
        // A transient failure on one interface must not abort the whole lookup;
        // skip it and try the next rather than returning None (or panicking).
        let out = match Command::new("/usr/sbin/ipconfig")
            .arg("getifaddr")
            .arg(iface)
            .output()
        {
            Ok(out) => out,
            Err(_) => continue,
        };
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Single GET to a key-less echo service, with a hard timeout enforced by a
/// worker thread + `recv_timeout` so a stalled connection can never block us.
fn fetch_public_ip() -> Option<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = ureq::get("https://api.ipify.org")
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_to_string().ok());
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Some(s)) => {
            let ip = s.trim().to_string();
            if ip.is_empty() {
                None
            } else {
                Some(ip)
            }
        }
        _ => None,
    }
}

fn push_listening_ports(out: &mut Vec<Item>) {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN"])
        .output();
    let Ok(output) = output else {
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut seen = std::collections::HashSet::new();
    let mut count = 0;
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let proc = fields[0];
        let pid = fields[1];
        let addr = fields[8];
        let port = addr.rsplit(':').next().unwrap_or(addr);
        let key = format!("{proc}:{port}");
        if !seen.insert(key) {
            continue;
        }
        out.push(Item::new(
            format!("Port {port} - {proc}"),
            format!("pid {pid}, {addr} - Enter to copy port"),
            "Network",
            9_100 - count,
            Action::CopyText(port.to_string()),
        ));
        count += 1;
        if count >= 12 {
            break;
        }
    }
    if count == 0 {
        out.push(Item::new(
            "No listening TCP ports",
            "Nothing is listening (or lsof returned nothing)",
            "Network",
            9_100,
            Action::None,
        ));
    }
}

fn push_port_lookup(out: &mut Vec<Item>, port: u16) {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output();
    let Ok(output) = output else {
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut found = false;
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        found = true;
        out.push(Item::new(
            format!("Port {port}: {} (pid {})", fields[0], fields[1]),
            "Process listening on this port - Enter to copy pid",
            "Network",
            9_200,
            Action::CopyText(fields[1].to_string()),
        ));
        break;
    }
    if !found {
        out.push(Item::new(
            format!("Port {port} is free"),
            "No process is listening on this port",
            "Network",
            9_200,
            Action::None,
        ));
    }
}

fn push_wifi_networks(out: &mut Vec<Item>) {
    let Some(device) = wifi_device() else {
        out.push(Item::new(
            "No Wi-Fi interface",
            "Could not find a Wi-Fi hardware port",
            "Network",
            9_000,
            Action::None,
        ));
        return;
    };
    let output = Command::new("/usr/sbin/networksetup")
        .arg("-listpreferredwirelessnetworks")
        .arg(&device)
        .output();
    let Ok(output) = output else {
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut count = 0;
    for line in text.lines().skip(1) {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        out.push(Item::new(
            name.to_string(),
            "Preferred Wi-Fi network - Enter to copy",
            "Network",
            9_000 - count,
            Action::CopyText(name.to_string()),
        ));
        count += 1;
        if count >= 15 {
            break;
        }
    }
    if count == 0 {
        out.push(Item::new(
            "No preferred Wi-Fi networks",
            "The list is empty or unavailable",
            "Network",
            9_000,
            Action::None,
        ));
    }
}

fn wifi_device() -> Option<String> {
    let output = Command::new("/usr/sbin/networksetup")
        .arg("-listallhardwareports")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.contains("Wi-Fi") || line.contains("AirPort") {
            for next in lines.by_ref() {
                if let Some(dev) = next.strip_prefix("Device: ") {
                    return Some(dev.trim().to_string());
                }
                if next.trim().is_empty() {
                    break;
                }
            }
        }
    }
    None
}
