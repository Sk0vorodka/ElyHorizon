//! Server Helper — a Horizon mod for offline servers with auth plugins
//! (`/login`, `/register`): it remembers your password locally and answers
//! the prompts itself, so joining a server is one click.
//!
//! Everything it does is reported in chat. The HUD is opt-in: `/shud` shows
//! a small one-line panel in the bottom-right corner (away from the chat,
//! the crosshair and the scoreboard). The only other moment anything is on
//! screen is the registration flash, which clears itself after a few
//! seconds — no marker follows the login attempts around.
//!
//! Commands:
//! - `/shud` — toggle the optional status panel (bottom-right corner).
//! - `/spass <password>` — store the password and arm auto-login.
//! - `/sgen [len]` — generate a random password, store it, and answer the
//!   server's `/register` with it (so you can log in from other launchers too).
//! - `/spatl <text>` — the chat text that counts as a login prompt
//!   (default: `/login`).
//! - `/spatr <text>` — the same for registration (default: `/reg`, which
//!   matches `/reg` and `/register`).
//! - `/sforget` — forget the password.
//! - `/shelp` — this list.
//!
//! Everything lives in the mod's own storage on this computer.

use rustcraft_sdk::{
    Mod, Registration,
    chat, command,
    hud::{self, Corner, Line},
    storage,
};

const PASS_KEY: &str = "password";
const PATTERN_LOGIN_KEY: &str = "pattern_login";
const PATTERN_REGISTER_KEY: &str = "pattern_register";
const ENABLED_KEY: &str = "enabled";
const PANEL_KEY: &str = "panel";
const DEFAULT_LOGIN_PATTERN: &str = "/login";
const DEFAULT_REGISTER_PATTERN: &str = "/reg";
const MAX_LOGIN_ATTEMPTS: u32 = 3;
/// How long the registration flash stays up by itself (4 s): after that the
/// screen is clean again, whether the server accepted it or not — chat
/// carries the news either way.
const FLASH_TICKS: u64 = 80;

struct ServerHelper {
    /// Auto-login answers the prompts (default on).
    on: bool,
    /// The optional status panel, bottom-right (default off; `/shud`).
    panel: bool,
    password: Option<String>,
    /// The login prompt's text (`/login`).
    pat_login: String,
    /// The register prompt's text (`/reg` matches `/reg` and `/register`).
    pat_reg: String,
    /// Login attempts spent on the current stay, so a wrong password cannot
    /// loop.
    login_attempts: u32,
    /// Registered on this server during this session.
    registered: bool,
    /// The one-line flash on screen, and the tick it disappears after.
    flash: String,
    flash_until: u64,
    /// The last tick seen (a chat answer times its flash from it).
    last_tick: u64,
    last_shown: Option<Vec<String>>,
}

/// The prompt test: the stored pattern, case-insensitively, anywhere in the line.
fn is_prompt(pattern: &str, line: &str) -> bool {
    line.to_lowercase().contains(&pattern.to_lowercase())
}

/// Advances on every [`generate`] call, so two passwords made in one tick
/// still differ (the counter's odd start keeps the xorshift's `| 1` intact).
static CALLS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// A random alphanumeric password (no look-alikes: no 0/O, 1/l/I).
///
/// The sandbox offers no wall clock and no randomness (both panic or miss on
/// `wasm32-unknown-unknown`), so the state mixes the tick the mod last saw
/// with a per-call counter, through splitmix64's finalizer — adjacent
/// counter values land far apart, unlike a plain `| 1` mask that could
/// swallow a one-bit difference. Fine for a server password a player can
/// also type by hand; not cryptography.
fn generate(len: usize, tick: u64) -> String {
    let alphabet: &[u8] = b"abcdefghjkmnpqrstuvwxyzACDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut z = tick ^ CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    let mut x = z ^ (z >> 31);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            if x == 0 {
                x = 0x2545_F491_4F6C_DD1D; // the xorshift's fixed point
            }
            alphabet[(x % alphabet.len() as u64) as usize] as char
        })
        .collect()
}

/// The panel's single line: it sits in the corner during play, so it stays
/// short — the details are `/shelp` and the chat.
fn lines(helper: &ServerHelper) -> Vec<String> {
    vec![format!(
        "Server Helper: {}",
        match &helper.password {
            Some(_) => "armed (/shelp)",
            None => "no password: /sgen or /spass",
        }
    )]
}

/// What the HUD shows this tick, if anything: a flash in progress, else the
/// optional panel — and during normal play, nothing at all.
fn hud_now(helper: &ServerHelper, tick: u64) -> Option<Vec<String>> {
    if tick < helper.flash_until {
        return Some(vec![helper.flash.clone()]);
    }
    if helper.panel {
        return Some(lines(helper));
    }
    None
}

/// Puts `hud_now` on screen, in the bottom-right corner, only when it changed.
fn draw(helper: &mut ServerHelper, tick: u64) {
    let now = hud_now(helper, tick);
    if now != helper.last_shown {
        if let Some(ls) = &now {
            let panel: Vec<Line> = ls
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::BottomRight, &panel);
        } else {
            hud::clear();
        }
        helper.last_shown = now;
    }
}

/// Shows a one-line flash that clears itself after [`FLASH_TICKS`].
fn flash(helper: &mut ServerHelper, text: &str) {
    helper.flash = text.to_owned();
    helper.flash_until = helper.last_tick.saturating_add(FLASH_TICKS);
}

fn stored_pattern(key: &str, default: &str) -> String {
    storage::get(key)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_else(|| default.to_owned())
}

fn stored_password() -> Option<String> {
    storage::get(PASS_KEY).map(|b| String::from_utf8_lossy(&b).into_owned())
}

impl Mod for ServerHelper {
    fn init() -> Result<(Self, Registration), String> {
        let password = stored_password();
        let on = storage::get(ENABLED_KEY)
            .map(|b| b.first() == Some(&b'1'))
            .unwrap_or(true);
        let panel = storage::get(PANEL_KEY)
            .map(|b| b.first() == Some(&b'1'))
            .unwrap_or(false);
        Ok((
            ServerHelper {
                on,
                panel,
                password,
                pat_login: stored_pattern(PATTERN_LOGIN_KEY, DEFAULT_LOGIN_PATTERN),
                pat_reg: stored_pattern(PATTERN_REGISTER_KEY, DEFAULT_REGISTER_PATTERN),
                login_attempts: 0,
                registered: false,
                flash: String::new(),
                flash_until: 0,
                last_tick: 0,
                last_shown: None,
            },
            Registration::new()
                .command("shud")
                .command("spass")
                .command("sgen")
                .command("spatl")
                .command("spatr")
                .command("sforget")
                .command("shelp")
                .on_tick()
                .on_chat(),
        ))
    }

    fn tick(&mut self, tick: u64) {
        self.last_tick = tick;
        draw(self, tick);
    }

    fn chat(&mut self, text: &str) {
        if !self.on {
            return;
        }
        let low = text.to_lowercase();
        // The register prompt first: a new player meets it before /login
        // works. Without a stored password one is generated and shown.
        if is_prompt(&self.pat_reg, &low) && !self.registered {
            let pass = match &self.password {
                Some(p) => p.clone(),
                None => {
                    let p = generate(12, self.last_tick);
                    storage::set(PASS_KEY, p.as_bytes()).ok();
                    self.password = Some(p.clone());
                    chat::show(&format!(
                        "Server Helper: generated your password: {p} — write it down."
                    ));
                    p
                }
            };
            // As typed: the server's plugin command (run carries vanilla
            // commands only).
            command::send(&format!("/register {pass} {pass}"));
            self.registered = true;
            flash(self, "Server Helper: registering…");
            chat::show("Server Helper: registered. The next prompt is a /login.");
            return;
        }
        if is_prompt(&self.pat_login, &low) {
            if self.login_attempts >= MAX_LOGIN_ATTEMPTS {
                return;
            }
            let Some(pass) = &self.password else {
                if !self.registered {
                    chat::show(
                        "Server Helper: no password yet — /sgen to register, /spass to set.",
                    );
                }
                return;
            };
            self.login_attempts += 1;
            // As typed: the server's plugin command (run carries vanilla
            // commands only).
            command::send(&format!("/login {pass}"));
            chat::show(&format!(
                "Server Helper: /login sent (attempt {} of {MAX_LOGIN_ATTEMPTS}).",
                self.login_attempts
            ));
        }
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        match name {
            "shud" => {
                self.panel = !self.panel;
                storage::set(PANEL_KEY, if self.panel { b"1" } else { b"0" }).ok();
                if !self.panel {
                    hud::clear();
                    self.last_shown = None;
                }
                Ok(Some(format!(
                    "Status panel {} — reports stay in chat either way.",
                    if self.panel {
                        "on (bottom-right corner)"
                    } else {
                        "off"
                    }
                )))
            }
            "spass" => {
                let pass = args.trim();
                if pass.is_empty() {
                    return Err("Usage: /spass <password>".into());
                }
                storage::set(PASS_KEY, pass.as_bytes()).ok();
                self.password = Some(pass.to_owned());
                self.login_attempts = 0;
                Ok(Some("Password stored; auto-login is armed.".into()))
            }
            "sgen" => {
                let len = args.trim().parse::<usize>().unwrap_or(12).clamp(8, 32);
                let pass = generate(len, self.last_tick);
                storage::set(PASS_KEY, pass.as_bytes()).ok();
                self.password = Some(pass.clone());
                self.login_attempts = 0;
                Ok(Some(format!(
                    "Generated and stored: {pass} — write it down if you log in from elsewhere."
                )))
            }
            "spatl" => {
                let text = args.trim();
                if text.is_empty() {
                    return Err("Usage: /spatl <prompt text>".into());
                }
                storage::set(PATTERN_LOGIN_KEY, text.as_bytes()).ok();
                self.pat_login = text.to_owned();
                Ok(Some(format!("Login prompt: {text}")))
            }
            "spatr" => {
                let text = args.trim();
                if text.is_empty() {
                    return Err("Usage: /spatr <prompt text>".into());
                }
                storage::set(PATTERN_REGISTER_KEY, text.as_bytes()).ok();
                self.pat_reg = text.to_owned();
                Ok(Some(format!("Register prompt: {text}")))
            }
            "sforget" => {
                storage::delete(PASS_KEY);
                self.password = None;
                Ok(Some("Password forgotten.".into()))
            }
            "shelp" => Ok(Some(
                "/spass <password> — store and arm; /sgen [len] — generate and store; /spatl|/spatr <text> — the login/register prompt texts; /sforget — forget; /shud — a small panel in the bottom-right corner (off by default).".into(),
            )),
            _ => Err("This command does nothing".into()),
        }
    }
}

rustcraft_sdk::export_mod!(ServerHelper);

#[cfg(test)]
mod tests {
    use super::*;

    fn helper() -> ServerHelper {
        ServerHelper {
            on: true,
            panel: false,
            password: Some("hunter2".into()),
            pat_login: "/login".into(),
            pat_reg: "/reg".into(),
            login_attempts: 1,
            registered: false,
            flash: String::new(),
            flash_until: 0,
            last_tick: 0,
            last_shown: None,
        }
    }

    #[test]
    fn the_prompt_matches_case_insensitively() {
        assert!(is_prompt("/login", "Используйте команду /LOGIN <пароль>"));
        assert!(is_prompt("/login", "type /login to continue"));
        assert!(!is_prompt("/login", "welcome to the server"));
    }

    #[test]
    fn register_prompt_matches_the_register_pattern() {
        assert!(is_prompt("/reg", "Вы не зарегистрированы: /register <pass>"));
        assert!(is_prompt("/reg", "Please /register first"));
        assert!(!is_prompt("/reg", "Use /login <password>"));
    }

    #[test]
    fn generated_passwords_use_the_safe_alphabet() {
        let pass = generate(12, 1_000);
        assert_eq!(pass.len(), 12);
        assert!(pass.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(!pass.chars().any(|c| "ilo01O".contains(c)));
    }

    #[test]
    fn two_passwords_in_one_tick_differ() {
        let a = generate(16, 1_000);
        let b = generate(16, 1_000);
        assert_ne!(a, b);
        assert_ne!(generate(16, 0), generate(16, u64::MAX));
    }

    #[test]
    fn the_panel_is_one_short_line() {
        let mut helper = helper();
        assert_eq!(lines(&helper), vec!["Server Helper: armed (/shelp)"]);
        helper.password = None;
        assert_eq!(
            lines(&helper),
            vec!["Server Helper: no password: /sgen or /spass"]
        );
    }

    #[test]
    fn the_hud_stays_quiet_during_play() {
        let mut helper = helper();
        // Normal play, panel off: nothing on screen at all.
        assert_eq!(hud_now(&helper, 100), None);
        // The registration flash: it shows, then clears itself — no success
        // detection needed.
        flash(&mut helper, "Server Helper: registering…");
        assert_eq!(
            hud_now(&helper, helper.last_tick),
            Some(vec!["Server Helper: registering…".to_owned()])
        );
        assert_eq!(hud_now(&helper, helper.flash_until), None);
        // The opt-in panel shows while it is on.
        helper.panel = true;
        assert!(hud_now(&helper, helper.flash_until).is_some());
    }
}
