use serde::Serialize;
use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::net::UdpSocket;

#[derive(Serialize)]
pub struct DiscoveredPrinter {
    pub ip: String,
    pub serial: String,
    model: String,
}
fn parse(data: &[u8], sender: SocketAddr) -> Option<DiscoveredPrinter> {
    let text = std::str::from_utf8(data).ok()?;
    let headers: BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    if !text.to_ascii_lowercase().contains("bambu") && !headers.contains_key("devmodel.bambu.com") {
        return None;
    }
    let serial = headers
        .get("usn")?
        .trim_start_matches("uuid:")
        .split("::")
        .next()?
        .to_string();
    if serial.len() < 6 || serial.len() > 32 || !serial.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let ip = match sender.ip() {
        std::net::IpAddr::V4(ip) if ip.is_private() || ip.is_link_local() => ip.to_string(),
        _ => return None,
    };
    let model = headers
        .get("devmodel.bambu.com")
        .cloned()
        .unwrap_or_else(|| "Bambu printer".into());
    let model = match model.as_str() {
        "N7" => "P2S".into(),
        _ => model
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
            .take(24)
            .collect(),
    };
    Some(DiscoveredPrinter { ip, serial, model })
}
#[tauri::command]
pub async fn discover() -> Result<Vec<DiscoveredPrinter>, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 2021))
        .await
        .map_err(|_| "discovery-unavailable")?;
    socket
        .join_multicast_v4(Ipv4Addr::new(239, 255, 255, 250), Ipv4Addr::UNSPECIFIED)
        .map_err(|_| "discovery-unavailable")?;
    let request=b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:2021\r\nMAN: \"ssdp:discover\"\r\nMX: 3\r\nST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";
    let _ = socket
        .send_to(request, (Ipv4Addr::new(239, 255, 255, 250), 2021))
        .await;
    let end = tokio::time::Instant::now() + Duration::from_secs(7);
    let mut found = BTreeMap::new();
    let mut buffer = [0u8; 8192];
    while let Ok(Ok((n, sender))) =
        tokio::time::timeout_at(end, socket.recv_from(&mut buffer)).await
    {
        if let Some(printer) = parse(&buffer[..n], sender) {
            found.insert(printer.serial.clone(), printer);
        }
    }
    Ok(found.into_values().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_unrelated_and_malformed_announcements() {
        let sender = "192.168.1.2:2021".parse().unwrap();
        assert!(parse(b"HTTP/1.1 200 OK\r\nUSN: unrelated\r\n", sender).is_none());
        let result = parse(
            b"HTTP/1.1 200 OK\r\nDevModel.bambu.com: N7\r\nUSN: TESTSERIAL123\r\n",
            sender,
        )
        .unwrap();
        assert_eq!(result.model, "P2S");
    }
}
