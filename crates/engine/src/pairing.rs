//! The code another device must give before it may control the lights.
//!
//! When the UI is opened to the network, a phone on the same WiFi can be
//! a remote. So can anyone else on that WiFi, unless something is asked
//! of them. What is asked is six digits shown on the computer itself. A
//! device that gives them is handed a long random token in a cookie and
//! is not asked again while the program runs.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The cookie a paired device carries.
pub const COOKIE: &str = "darkrgb_pair";
/// Wrong codes one device may give before it has to wait.
const TRIES: u32 = 5;
/// Wrong codes all devices together may give before everyone has to
/// wait. Guessing from many addresses at once is then no quicker.
const TRIES_BY_ALL: u32 = 30;
/// How long the wait is, and the span tries are counted over.
const WAIT: Duration = Duration::from_secs(60);
/// Devices remembered for counting tries.
const MAX_PEERS: usize = 256;

/// Wrong tries since `since`.
#[derive(Clone, Copy)]
struct Count {
    failed: u32,
    since: Instant,
}

impl Count {
    fn new(now: Instant) -> Self {
        Self { failed: 0, since: now }
    }

    /// Seconds left to wait, if `limit` has been reached.
    fn wait(&self, limit: u32, now: Instant) -> Option<u64> {
        let until = self.since + WAIT;
        (self.failed >= limit && now < until).then(|| (until - now).as_secs().max(1))
    }

    fn miss(&mut self, now: Instant) {
        if now >= self.since + WAIT {
            *self = Self::new(now);
        }
        self.failed += 1;
    }
}

struct Tries {
    by_peer: HashMap<IpAddr, Count>,
    all: Count,
}

/// Why a code was not accepted.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not the code. `left` more tries before a wait.
    Wrong { left: u32 },
    /// Too many wrong codes. Try again in this many seconds.
    Wait { seconds: u64 },
}

pub struct Pairing {
    code: String,
    token: String,
    tries: Mutex<Tries>,
}

impl std::fmt::Debug for Pairing {
    // Neither secret belongs in a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pairing")
    }
}

/// Compares without stopping at the first difference, so the time taken
/// says nothing about how much of a guess was right.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

impl Pairing {
    /// A new code and token from the operating system's random numbers.
    pub fn new() -> Result<Self, String> {
        let mut bytes = [0u8; 20];
        getrandom::fill(&mut bytes).map_err(|e| format!("the system gave no random numbers: {e}"))?;
        let number = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1_000_000;
        let token: String = bytes[4..].iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self::with(&format!("{number:06}"), &token))
    }

    /// For tests, and for nothing else.
    pub fn with(code: &str, token: &str) -> Self {
        Self {
            code: code.into(),
            token: token.into(),
            tries: Mutex::new(Tries {
                by_peer: HashMap::new(),
                all: Count::new(Instant::now()),
            }),
        }
    }

    /// The six digits to show on the computer.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Checks a code typed on the device at `peer`. On success, the token
    /// to hand it. Spaces and dashes in what was typed do not matter.
    pub fn try_code(&self, typed: &str, peer: IpAddr, now: Instant) -> Result<&str, Refused> {
        let typed: String = typed.chars().filter(char::is_ascii_digit).collect();
        let mut tries = self.tries.lock().unwrap();
        let own = tries.by_peer.get(&peer).copied().unwrap_or(Count::new(now));
        if let Some(seconds) = own.wait(TRIES, now).or(tries.all.wait(TRIES_BY_ALL, now)) {
            return Err(Refused::Wait { seconds });
        }
        if same(&typed, &self.code) {
            tries.by_peer.remove(&peer);
            return Ok(&self.token);
        }
        tries.all.miss(now);
        if tries.by_peer.len() >= MAX_PEERS {
            tries.by_peer.retain(|_, count| now < count.since + WAIT);
        }
        // With the list still full, this device is counted with everyone.
        let mut own = own;
        own.miss(now);
        if tries.by_peer.len() < MAX_PEERS || tries.by_peer.contains_key(&peer) {
            tries.by_peer.insert(peer, own);
        }
        match own.wait(TRIES, now).or(tries.all.wait(TRIES_BY_ALL, now)) {
            Some(seconds) => Err(Refused::Wait { seconds }),
            None => Err(Refused::Wrong {
                left: TRIES - own.failed,
            }),
        }
    }

    /// Whether a request's `Cookie` header carries the token.
    pub fn knows(&self, cookies: Option<&str>) -> bool {
        cookies.is_some_and(|cookies| {
            cookies
                .split(';')
                .filter_map(|pair| pair.trim().split_once('='))
                .any(|(name, value)| name == COOKIE && same(value, &self.token))
        })
    }

    /// The `Set-Cookie` value that pairs a device. Not readable by
    /// scripts, and never sent along with a request from another site.
    pub fn cookie(&self) -> String {
        format!(
            "{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000",
            self.token
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u8) -> IpAddr {
        IpAddr::from([192, 168, 1, n])
    }

    #[test]
    fn a_new_code_is_six_digits_and_the_token_is_long() {
        let a = Pairing::new().unwrap();
        let b = Pairing::new().unwrap();
        assert_eq!(a.code().len(), 6);
        assert!(a.code().chars().all(|c| c.is_ascii_digit()));
        assert_eq!(a.token.len(), 32);
        assert_ne!(a.token, b.token);
        assert_eq!(format!("{a:?}"), "Pairing", "secrets stay out of logs");
    }

    #[test]
    fn the_right_code_gives_the_token_however_it_is_typed() {
        let pairing = Pairing::with("048213", "tok");
        let now = Instant::now();
        assert_eq!(pairing.try_code("048213", peer(2), now), Ok("tok"));
        assert_eq!(pairing.try_code(" 048 213 ", peer(2), now), Ok("tok"));
        assert_eq!(pairing.try_code("048-213", peer(2), now), Ok("tok"));
        assert_eq!(pairing.try_code("48213", peer(2), now), Err(Refused::Wrong { left: 4 }));
        assert_eq!(pairing.try_code("", peer(2), now), Err(Refused::Wrong { left: 3 }));
        // Getting it right clears the count.
        assert_eq!(pairing.try_code("048213", peer(2), now), Ok("tok"));
        assert_eq!(pairing.try_code("1", peer(2), now), Err(Refused::Wrong { left: 4 }));
    }

    #[test]
    fn five_wrong_codes_mean_a_minutes_wait_for_that_device_only() {
        let pairing = Pairing::with("048213", "tok");
        let now = Instant::now();
        for left in (1..=4).rev() {
            assert_eq!(pairing.try_code("000000", peer(2), now), Err(Refused::Wrong { left }));
        }
        assert_eq!(
            pairing.try_code("000000", peer(2), now),
            Err(Refused::Wait { seconds: 60 })
        );
        // Even the right code has to wait.
        let later = now + Duration::from_secs(20);
        assert_eq!(
            pairing.try_code("048213", peer(2), later),
            Err(Refused::Wait { seconds: 40 })
        );
        // Another device is not held up.
        assert_eq!(pairing.try_code("048213", peer(3), later), Ok("tok"));
        // And after the minute the first may try again.
        let after = now + Duration::from_secs(61);
        assert_eq!(
            pairing.try_code("000000", peer(2), after),
            Err(Refused::Wrong { left: 4 })
        );
        assert_eq!(pairing.try_code("048213", peer(2), after), Ok("tok"));
    }

    #[test]
    fn guessing_from_many_addresses_makes_everyone_wait() {
        let pairing = Pairing::with("048213", "tok");
        let now = Instant::now();
        let mut refused = 0;
        for n in 0..40u8 {
            if matches!(pairing.try_code("000000", peer(n), now), Err(Refused::Wait { .. })) {
                refused += 1;
            }
        }
        assert_eq!(refused, 11, "the thirtieth wrong code and every one after it");
        assert!(matches!(
            pairing.try_code("048213", peer(200), now),
            Err(Refused::Wait { .. })
        ));
        assert_eq!(
            pairing.try_code("048213", peer(200), now + Duration::from_secs(61)),
            Ok("tok")
        );
        // The list of devices does not grow without end.
        let pairing = Pairing::with("048213", "tok");
        for n in 0..2000u32 {
            let ip = IpAddr::from(n.to_be_bytes());
            let _ = pairing.try_code("0", ip, now + Duration::from_secs(n as u64));
        }
        assert!(pairing.tries.lock().unwrap().by_peer.len() <= MAX_PEERS);
    }

    #[test]
    fn the_cookie_is_found_among_others_and_nothing_else_passes() {
        let pairing = Pairing::with("048213", "abc123");
        assert!(pairing.knows(Some("darkrgb_pair=abc123")));
        assert!(pairing.knows(Some("theme=dark; darkrgb_pair=abc123; x=y")));
        assert!(!pairing.knows(None));
        assert!(!pairing.knows(Some("")));
        assert!(!pairing.knows(Some("darkrgb_pair=abc12")));
        assert!(!pairing.knows(Some("darkrgb_pair=abc1234")));
        assert!(!pairing.knows(Some("other=abc123")));
        assert!(!pairing.knows(Some("darkrgb_pair=048213")), "the code is not the token");
        let cookie = pairing.cookie();
        assert!(cookie.starts_with("darkrgb_pair=abc123;"));
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    }
}
