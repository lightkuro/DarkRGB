//! Runs one or more pretend WLED devices.
//!
//! ```text
//! ols-sim --device "Desk strip:120" --device "Panel:16x16"
//! ```

use std::time::Duration;

use clap::Parser;
use ols_sim::{Sim, SimConfig};

#[derive(Parser)]
#[command(version, about = "Pretend WLED devices for testing OpenLightShow without hardware")]
struct Args {
    /// A device as NAME:COUNT for a strip or NAME:COLSxROWS for a matrix.
    /// Repeat for several devices.
    #[arg(long = "device", value_name = "NAME:SIZE", default_values = ["Sim strip:120", "Sim panel:16x16"])]
    devices: Vec<String>,

    /// HTTP port of the first device; each further device uses the next one.
    #[arg(long, default_value_t = 8081)]
    http_port: u16,

    /// DDP port of the first device; each further device uses the next one.
    #[arg(long, default_value_t = 14048)]
    ddp_port: u16,

    /// Do not advertise on the network. Add devices by address instead.
    #[arg(long)]
    no_mdns: bool,

    /// Accept connections from other computers, not only this one.
    #[arg(long)]
    public: bool,
}

fn parse_device(spec: &str) -> Result<(String, u32, Option<(u32, u32)>), String> {
    let (name, size) = spec
        .rsplit_once(':')
        .ok_or_else(|| format!("{spec:?} should look like NAME:120 or NAME:16x16"))?;
    let bad = || format!("{size:?} is not a size; use a count like 120 or a grid like 16x16");
    let (leds, matrix) = match size.split_once(['x', 'X']) {
        Some((w, h)) => {
            let (w, h): (u32, u32) = (w.parse().map_err(|_| bad())?, h.parse().map_err(|_| bad())?);
            (w.checked_mul(h).ok_or_else(bad)?, Some((w, h)))
        }
        None => (size.parse().map_err(|_| bad())?, None),
    };
    if leds == 0 || leds > 100_000 {
        return Err(format!("{name}: the LED count must be between 1 and 100000"));
    }
    Ok((name.to_string(), leds, matrix))
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let mut sims = Vec::new();
    for (i, spec) in args.devices.iter().enumerate() {
        let (name, leds, matrix) = match parse_device(spec) {
            Ok(parsed) => parsed,
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(2);
            }
        };
        let config = SimConfig {
            name,
            leds,
            matrix,
            http_port: args.http_port + i as u16,
            ddp_port: args.ddp_port + i as u16,
            mdns: !args.no_mdns,
            public: args.public,
        };
        match Sim::start(config) {
            Ok(sim) => {
                println!(
                    "{}: {} LEDs, add it by address 127.0.0.1:{} (DDP on {})",
                    sim.name, leds, sim.http_port, sim.ddp_port
                );
                sims.push(sim);
            }
            Err(e) => {
                eprintln!("could not start {spec}: {e}");
                std::process::exit(1);
            }
        }
    }

    loop {
        std::thread::sleep(Duration::from_secs(5));
        for sim in &sims {
            let s = sim.stats();
            if s.frames > 0 {
                println!(
                    "{}: {:.1} fps, frame gap {:.1} ms (min {:.1}, max {:.1}, jitter {:.2}), {} frames",
                    sim.name, s.fps, s.interval_ms, s.min_ms, s.max_ms, s.jitter_ms, s.frames
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_device;

    #[test]
    fn parses_strips_and_matrices() {
        assert_eq!(parse_device("Desk:120").unwrap(), ("Desk".into(), 120, None));
        assert_eq!(parse_device("Panel:16x8").unwrap(), ("Panel".into(), 128, Some((16, 8))));
        assert_eq!(parse_device("A:B:5").unwrap(), ("A:B".into(), 5, None));
        assert!(parse_device("no size").is_err());
        assert!(parse_device("x:0").is_err());
        assert!(parse_device("x:4xfour").is_err());
        assert!(parse_device("x:99999x99999").is_err());
    }
}
