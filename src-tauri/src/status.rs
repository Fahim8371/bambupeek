use crate::{Connection, StreamEvent};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    ClientConfig, DigitallySignedStruct, SignatureScheme,
};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tauri::ipc::Channel;

// LAN printers ship self-signed certificates. This TLS exception is scoped to the printer
// connection; it does not change system-wide or webview certificate validation.
#[derive(Debug)]
struct PrinterCertificate;
impl ServerCertVerifier for PrinterCertificate {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintStatus {
    state: Option<String>,
    progress: Option<u64>,
    layer: Option<u64>,
    total_layers: Option<u64>,
    remaining_minutes: Option<u64>,
    nozzle: Option<f64>,
    bed: Option<f64>,
}
impl PrintStatus {
    fn merge(&mut self, v: &Value) {
        if let Some(s) = v["gcode_state"].as_str() {
            self.state = Some(
                match s {
                    "RUNNING" => "Printing",
                    "PAUSE" => "Paused",
                    "FINISH" => "Complete",
                    "FAILED" => "Print failed",
                    "IDLE" => "Idle",
                    "PREPARE" => "Preparing",
                    _ => "Ready",
                }
                .into(),
            );
        }
        if let Some(n) = v["mc_percent"].as_u64() {
            self.progress = Some(n.min(100));
        }
        if let Some(n) = v["layer_num"].as_u64() {
            self.layer = Some(n);
        }
        if let Some(n) = v["total_layer_num"].as_u64() {
            self.total_layers = Some(n);
        }
        if let Some(n) = v["mc_remaining_time"].as_u64() {
            self.remaining_minutes = Some(n);
        }
        if let Some(n) = v["nozzle_temper"].as_f64() {
            self.nozzle = Some(n);
        }
        if let Some(n) = v["bed_temper"].as_f64() {
            self.bed = Some(n);
        }
    }
}
pub async fn run(config: Connection, events: Channel<StreamEvent>) {
    let tls =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PrinterCertificate))
            .with_no_client_auth();
    let client_id = format!("bambupeek-{}", std::process::id());
    let mut options = MqttOptions::new(client_id, &config.ip, 8883);
    options.set_credentials("bblp", &config.access_code);
    options.set_keep_alive(Duration::from_secs(15));
    // Full reports can include AMS and device details in addition to the fields we display.
    options.set_max_packet_size(1024 * 1024, 64 * 1024);
    options.set_transport(Transport::Tls(TlsConfiguration::Rustls(Arc::new(tls))));
    let (client, mut eventloop) = AsyncClient::new(options, 10);
    let report = format!("device/{}/report", config.serial);
    let request = format!("device/{}/request", config.serial);
    let mut snapshot = PrintStatus::default();
    let mut last_report = tokio::time::Instant::now();
    let mut warned = false;
    loop {
        match tokio::time::timeout(Duration::from_secs(30), eventloop.poll()).await {
            Ok(Ok(Event::Incoming(Packet::ConnAck(_)))) => {
                if client.subscribe(&report, QoS::AtMostOnce).await.is_err() {
                    break;
                }
                last_report = tokio::time::Instant::now();
                warned = false;
            }
            Ok(Ok(Event::Incoming(Packet::SubAck(ack)))) => {
                if ack
                    .return_codes
                    .iter()
                    .any(|code| matches!(code, rumqttc::SubscribeReasonCode::Failure))
                {
                    let _ = events.send(StreamEvent::StatusUnavailable {
                        reason: "subscription-rejected".into(),
                    });
                } else {
                    let _ = client
                        .publish(
                            &request,
                            QoS::AtMostOnce,
                            false,
                            r#"{"pushing":{"sequence_id":"1","command":"pushall"}}"#,
                        )
                        .await;
                }
            }
            Ok(Ok(Event::Incoming(Packet::Publish(packet)))) => {
                if packet.topic == report {
                    if let Ok(v) = serde_json::from_slice::<Value>(&packet.payload) {
                        if v["print"].is_object() {
                            last_report = tokio::time::Instant::now();
                            warned = false;
                            snapshot.merge(&v["print"]);
                            if events
                                .send(StreamEvent::Status {
                                    values: snapshot.clone(),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                // Map errors to fixed codes; do not expose raw addresses, credentials or payloads.
                let reason = classify_error(&error.to_string());
                let _ = events.send(StreamEvent::StatusUnavailable {
                    reason: reason.into(),
                });
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(_) => {
                let _ = events.send(StreamEvent::StatusUnavailable {
                    reason: "status-timeout".into(),
                });
            }
        }
        if !warned && last_report.elapsed() > Duration::from_secs(25) {
            warned = true;
            let _ = events.send(StreamEvent::StatusUnavailable {
                reason: "no-reports".into(),
            });
        }
    }
}
fn classify_error(raw: &str) -> &'static str {
    let text = raw.to_ascii_lowercase();
    if text.contains("notauthorized")
        || text.contains("badusernamepassword")
        || text.contains("not authorized")
    {
        "access-rejected"
    } else if text.contains("tls") || text.contains("certificate") || text.contains("handshake") {
        "tls-failed"
    } else if text.contains("no route") || text.contains("connection refused") {
        "network-blocked"
    } else if text.contains("timeout") {
        "status-timeout"
    } else {
        "connection-lost"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_reports_preserve_existing_status_and_omit_identity() {
        let mut s = PrintStatus::default();
        s.merge(&serde_json::json!({"gcode_state":"RUNNING","mc_percent":42,"gcode_file":"private-name.3mf"}));
        s.merge(&serde_json::json!({"nozzle_temper":220.0}));
        assert_eq!(s.progress, Some(42));
        assert_eq!(s.nozzle, Some(220.0));
        assert!(!serde_json::to_string(&s).unwrap().contains("private-name"));
    }
}
