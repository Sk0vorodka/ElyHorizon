//! Server Helper — a Horizon mod for offline servers with auth plugins
//! (`/login`, `/register`): it remembers your password locally and answers
//! the prompts itself, so joining a server is one click.
//!
//! Commands:
//! - `/shud on|off` — show/hide the status panel.
//! - `/spass <password>` — store the password and arm auto-login.
//! - `/sgen [len]` — generate a random password, store it, and answer the
//!   server's `/register` with it (so you can log in from other launchers too).
//! - `/spat <text>` — the chat text that counts as a login/register prompt
//!   (default: `/login`).
//! - `/sforget` — forget the password.
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
const DEFAULT_LOGIN_PATTERN: &str = "/login";
const DEFAULT_REGISTER_PATTERN: &str = "/reg";
const MAX_LOGIN_ATTEMPTS: u32 = 3;

struct ServerHelper {
    on: bool,
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
    last_panel: Vec<String>,
}

/// The prompt test: the stored pattern, case-insensitively, anywhere in the line.
fn is_prompt(pattern: &str, line: &str) -> bool {
    line.to_lowercase().contains(&pattern.to_lowercase())
}

/// A random alphanumeric password (no look-alikes: no 0/O, 1/l/I).
fn generate(len: usize) -> String {
    let alphabet: &[u8] = b"abcdefghjkmnpqrstuvwxyzACDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut x = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0x9E37_79B9)
        | 1)
        .wrapping_mul(0x2545_F491_4F6C_DD1D);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            alphabet[(x % alphabet.len() as u64) as usize] as char
        })
        .collect()
}

/// The panel's lines for the current state.
fn lines(helper: &ServerHelper) -> Vec<String> {
    let mut out = vec![format!(
        "Auto-login: {}",
        match (&helper.password, helper.on) {
            (Some(_), true) => "armed".to_string(),
            (Some(_), false) => "off".to_string(),
            (None, _) => "no password (/sgen to register, /spass to set)".to_string(),
        }
    )];
    if helper.password.is_some() {
        out.push(format!(
            "Login attempts left: {}",
            MAX_LOGIN_ATTEMPTS.saturating_sub(helper.login_attempts)
        ));
        out.push(format!("Prompts: {} | {}", helper.pat_login, helper.pat_reg));
    }
    out
}

fn draw_panel(helper: &mut ServerHelper) {
    let now = lines(helper);
    if now != helper.last_panel {
        let panel: Vec<Line> = now
            .iter()
            .map(|text| Line {
                text: text.clone(),
                color: 0xFFFFFF,
            })
            .collect();
        hud::set_panel(Corner::TopLeft, &panel);
        helper.last_panel = now;
    }
}

fn clear_panel(helper: &mut ServerHelper) {
    hud::clear();
    helper.last_panel.clear();
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
        Ok((
            ServerHelper {
                on,
                password,
                pat_login: stored_pattern(PATTERN_LOGIN_KEY, DEFAULT_LOGIN_PATTERN),
                pat_reg: stored_pattern(PATTERN_REGISTER_KEY, DEFAULT_REGISTER_PATTERN),
                login_attempts: 0,
                registered: false,
                last_panel: Vec::new(),
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

    fn tick(&mut self, _tick: u64) {
        if self.on {
            draw_panel(self);
        }
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
                    let p = generate(12);
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
                self.on = !self.on;
                if !self.on {
                    clear_panel(self);
                }
                storage::set(ENABLED_KEY, if self.on { b"1" } else { b"0" }).ok();
                Ok(Some(format!(
                    "Status panel {}",
                    if self.on { "on" } else { "off" }
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
                let pass = generate(len);
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
                "/spass <password> — store and arm; /sgen [len] — generate and store; /spatl|spatr <text> — the login/register prompt texts; /sforget — forget; /shud — the panel.".into(),
            )),
            _ => Err("This command does nothing".into()),
        }
    }
}

rustcraft_sdk::export_mod!(ServerHelper);

#[cfg(test)]
mod tests {
    use super::*;

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
        let pass = generate(12);
        assert_eq!(pass.len(), 12);
        assert!(pass.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(!pass.chars().any(|c| "ilo01O".contains(c)));
    }

    #[test]
    fn the_panel_lines_follow_the_state() {
        let mut helper = ServerHelper {
            on: true,
            password: Some("hunter2".into()),
            pat_login: "/login".into(),
            pat_reg: "/reg".into(),
            login_attempts: 1,
            registered: false,
            last_panel: Vec::new(),
        };
        let panel = lines(&helper);
        assert_eq!(panel[0], "Auto-login: armed");
        assert_eq!(panel[1], "Login attempts left: 2");
        assert_eq!(panel[2], "Prompts: /login | /reg");
        assert_eq!(lines(&helper).len(), 3);
        helper.password = None;
        assert_eq!(
            lines(&helper)[0],
            "Auto-login: no password (/sgen to register, /spass to set)"
        );
    }
}
