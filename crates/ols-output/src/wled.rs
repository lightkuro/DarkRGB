//! The small part of WLED's HTTP API needed to set a device up.

use std::time::Duration;

use serde::Deserialize;

use crate::ddp::DDP_PORT;

/// What `/json/info` tells us about a device.
#[derive(Clone, Debug, PartialEq)]
pub struct WledInfo {
    pub name: String,
    pub version: String,
    pub mac: Option<String>,
    pub arch: Option<String>,
    pub led_count: u32,
    /// Columns and rows, when the device is set up as a 2D matrix.
    pub matrix: Option<(u32, u32)>,
    /// True when something is already streaming to the device.
    pub live: bool,
    /// Who is streaming, as WLED describes it (protocol and address).
    pub live_source: Option<String>,
    /// UDP port for DDP. Always 4048 on real hardware; the simulator
    /// reports its own so several can share one machine.
    pub ddp_port: u16,
}

#[derive(Deserialize)]
struct RawInfo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ver: String,
    mac: Option<String>,
    arch: Option<String>,
    leds: RawLeds,
    #[serde(default)]
    live: bool,
    lm: Option<String>,
    lip: Option<String>,
    ols_sim: Option<RawSim>,
}

#[derive(Deserialize)]
struct RawLeds {
    count: u32,
    matrix: Option<RawMatrix>,
}

#[derive(Deserialize)]
struct RawMatrix {
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
struct RawSim {
    ddp_port: u16,
}

/// Parses the body of `/json/info`.
pub fn parse_info(json: &str) -> Result<WledInfo, String> {
    let raw: RawInfo = serde_json::from_str(json).map_err(|e| format!("not a WLED info reply: {e}"))?;
    if raw.leds.count == 0 {
        return Err("the device reports zero LEDs".into());
    }
    let live_source = match (raw.lm.filter(|s| !s.is_empty()), raw.lip.filter(|s| !s.is_empty())) {
        (Some(mode), Some(ip)) => Some(format!("{mode} from {ip}")),
        (Some(mode), None) => Some(mode),
        (None, Some(ip)) => Some(ip),
        (None, None) => None,
    };
    Ok(WledInfo {
        name: if raw.name.is_empty() { "WLED".into() } else { raw.name },
        version: raw.ver,
        mac: raw.mac.filter(|m| !m.is_empty()),
        arch: raw.arch,
        led_count: raw.leds.count,
        matrix: raw.leds.matrix.filter(|m| m.w > 1 && m.h > 1).map(|m| (m.w, m.h)),
        live: raw.live,
        live_source,
        ddp_port: raw.ols_sim.map(|s| s.ddp_port).unwrap_or(DDP_PORT),
    })
}

/// Splits what a user typed (`192.168.1.50`, `wled-desk.local:80`,
/// `http://192.168.1.50/`) into a host and an HTTP port.
pub fn parse_address(input: &str) -> Result<(String, u16), String> {
    let s = input.trim();
    let s = s.strip_prefix("http://").unwrap_or(s);
    if s.starts_with("https://") {
        return Err("WLED does not use https; enter the address without it".into());
    }
    let s = s.split('/').next().unwrap_or("");
    if s.is_empty() {
        return Err("enter an IP address or host name".into());
    }
    let (host, port) = match s.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().map_err(|_| format!("bad port {port:?}"))?),
        None => (s, 80),
    };
    let ok = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !ok {
        return Err(format!("{host:?} is not a valid address"));
    }
    Ok((host.to_string(), port))
}

/// Asks a device to describe itself. Blocks for at most `timeout`.
pub fn fetch_info(host: &str, http_port: u16, timeout: Duration) -> Result<WledInfo, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let url = format!("http://{host}:{http_port}/json/info");
    let body = agent
        .get(&url)
        .call()
        .map_err(|e| format!("no answer from {host}: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("bad reply from {host}: {e}"))?;
    parse_info(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real WLED 0.15 reply.
    const STRIP: &str = r#"{"ver":"0.15.0","vid":2412100,"leds":{"count":144,"pwr":0,"fps":42,"maxpwr":0,"maxseg":32,
        "seglc":[1],"lc":1,"rgbw":false,"wv":0,"cct":0},"str":false,"name":"Desk strip","udpport":21324,
        "live":false,"liveseg":-1,"lm":"","lip":"","ws":0,"fxcount":187,"palcount":71,
        "arch":"esp32","core":"v4.4.6","mac":"a0b1c2d3e4f5","ip":"192.168.1.50"}"#;

    const MATRIX: &str = r#"{"ver":"0.15.0","leds":{"count":256,"matrix":{"w":16,"h":16}},"name":"Panel",
        "live":true,"lm":"DDP","lip":"192.168.1.9","arch":"esp32","mac":"001122334455"}"#;

    #[test]
    fn reads_a_strip() {
        let info = parse_info(STRIP).unwrap();
        assert_eq!(info.name, "Desk strip");
        assert_eq!(info.version, "0.15.0");
        assert_eq!(info.led_count, 144);
        assert_eq!(info.matrix, None);
        assert_eq!(info.mac.as_deref(), Some("a0b1c2d3e4f5"));
        assert!(!info.live);
        assert_eq!(info.live_source, None);
        assert_eq!(info.ddp_port, 4048);
    }

    #[test]
    fn reads_a_matrix_that_is_already_being_streamed_to() {
        let info = parse_info(MATRIX).unwrap();
        assert_eq!(info.matrix, Some((16, 16)));
        assert!(info.live);
        assert_eq!(info.live_source.as_deref(), Some("DDP from 192.168.1.9"));
    }

    #[test]
    fn rejects_other_devices() {
        assert!(parse_info("<html>router login</html>").is_err());
        assert!(parse_info(r#"{"name":"x"}"#).is_err());
        assert!(parse_info(r#"{"leds":{"count":0}}"#).is_err());
    }

    #[test]
    fn simulator_can_report_its_own_ddp_port() {
        let info = parse_info(r#"{"leds":{"count":10},"ols_sim":{"ddp_port":14048}}"#).unwrap();
        assert_eq!(info.ddp_port, 14048);
        assert_eq!(info.name, "WLED");
    }

    #[test]
    fn parses_what_people_type() {
        assert_eq!(parse_address("192.168.1.50").unwrap(), ("192.168.1.50".into(), 80));
        assert_eq!(parse_address(" http://wled-desk.local/ ").unwrap(), ("wled-desk.local".into(), 80));
        assert_eq!(parse_address("127.0.0.1:8081").unwrap(), ("127.0.0.1".into(), 8081));
        assert!(parse_address("").is_err());
        assert!(parse_address("https://x").is_err());
        assert!(parse_address("a b").is_err());
        assert!(parse_address("host:notaport").is_err());
    }
}
