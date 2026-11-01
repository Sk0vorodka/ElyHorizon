//! Locus — a waypoint compass for Horizon.
//!
//! Save points under the block or where you stand, watch the HUD panel for
//! the nearest one, and teleport back with one command. Points live in the
//! mod's storage and survive reloads.
//!
//! Commands:
//! - `/locus tag <name>` — save your position as `<name>` (overwrites).
//! - `/locus go <name>` — teleport to it (`tp`, as if typed).
//! - `/locus list` — every point in chat.
//! - `/locus del <name>` — remove one.
//! - `/locus clear` — remove all.
//! - `/locus hud on|off` — show/hide the compass panel.

use rustcraft_sdk::{
    Mod, Registration, chat, command,
    game::{self, Player},
    hud::{self, Corner, Line},
    storage,
};

/// A saved point.
#[derive(Clone)]
struct Point {
    name: String,
    x: f64,
    y: f64,
    z: f64,
}

struct Locus {
    shown: bool,
    points: Vec<Point>,
    /// The panel last sent, so an unchanged one costs no call.
    last: Vec<String>,
}

/// The storage key holding every point, one `name|x|y|z` per line.
const KEY: &str = "waypoints";

fn encode(points: &[Point]) -> Vec<u8> {
    let mut out = String::new();
    for p in points {
        out.push_str(&format!("{}|{:.2}|{:.2}|{:.2}\n", p.name, p.x, p.y, p.z));
    }
    out.into_bytes()
}

fn decode(bytes: &[u8]) -> Vec<Point> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .filter_map(|line| {
            let mut it = line.split('|');
            let name = it.next()?.to_owned();
            let x = it.next()?.parse().ok()?;
            let y = it.next()?.parse().ok()?;
            let z = it.next()?.parse().ok()?;
            Some(Point { name, x, y, z })
        })
        .collect()
}

impl Locus {
    fn save(&mut self) {
        let _ = storage::set(KEY, &encode(&self.points));
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.points.iter().position(|p| p.name == name)
    }

    /// The horizontal distance to `p`, with the closest point's index.
    fn nearest(&self, p: &Player) -> Option<(usize, f64)> {
        self.points
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let dx = q.x - p.x;
                let dz = q.z - p.z;
                (i, (dx * dx + dz * dz).sqrt())
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// The compass line for one point, from the player's view.
    fn compass_line(&self, p: &Player, i: usize) -> String {
        let q = &self.points[i];
        let dx = q.x - p.x;
        let dz = q.z - p.z;
        let horiz = (dx * dx + dz * dz).sqrt();
        let dir = if horiz < 0.5 {
            "here".to_string()
        } else {
            let dirs = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
            let deg = dz.atan2(dx).to_degrees();
            let idx = ((deg + 22.5) / 45.0).rem_euclid(8.0) as usize;
            dirs[idx].to_string()
        };
        format!("{} {dir} {horiz:.0}m", q.name)
    }

    fn panel_lines(&self, p: &Player) -> Vec<String> {
        let Some((i, d)) = self.nearest(p) else {
            return vec!["Locus: no waypoints".into()];
        };
        let q = &self.points[i];
        let dy = q.y - p.y;
        let vertical = if dy.abs() < 1.5 {
            String::new()
        } else if dy > 0.0 {
            format!(" ↑{dy:.0}m")
        } else {
            format!(" ↓{dy:.0}m")
        };
        vec![
            format!("Locus • {}", q.name),
            format!("{}   distance {d:.0}m{vertical}", self.compass_line(p, i)),
        ]
    }
}

impl Mod for Locus {
    fn init() -> Result<(Self, Registration), String> {
        let points = storage::get(KEY).map(|b| decode(&b)).unwrap_or_default();
        let hooks = Registration::new().command("locus").on_tick();
        let state = Locus {
            shown: true,
            points,
            last: Vec::new(),
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        let Some(p) = game::get_player() else {
            return;
        };
        if !self.shown {
            return;
        }
        let lines = self.panel_lines(&p);
        if lines != self.last {
            let panel: Vec<Line> = lines
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFE082,
                })
                .collect();
            hud::set_panel(Corner::TopRight, &panel);
            self.last = lines;
        }
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        let mut it = args.split_whitespace();
        match it.next().unwrap_or("") {
            "tag" | "here" => {
                let label = it.collect::<Vec<_>>().join(" ");
                if label.is_empty() {
                    return Err("Usage: /locus tag <name>".into());
                }
                let p = game::get_player().ok_or("Not in a world")?;
                self.points.retain(|q| q.name != label);
                self.points.push(Point {
                    name: label.clone(),
                    x: p.x,
                    y: p.y,
                    z: p.z,
                });
                self.save();
                Ok(Some(format!("Waypoint \"{label}\" saved")))
            }
            "go" => {
                let label = it.collect::<Vec<_>>().join(" ");
                let i = self.find(&label).ok_or("No such waypoint")?;
                let q = self.points[i].clone();
                command::run(&format!("tp {:.2} {:.2} {:.2}", q.x, q.y, q.z));
                Ok(Some(format!("Jumping to \"{label}\"")))
            }
            "list" => {
                if self.points.is_empty() {
                    return Ok(Some("No waypoints yet".into()));
                }
                for q in &self.points {
                    chat::show(&format!("{}: {:.0}, {:.0}, {:.0}", q.name, q.x, q.y, q.z));
                }
                Ok(None)
            }
            "del" => {
                let label = it.collect::<Vec<_>>().join(" ");
                let before = self.points.len();
                self.points.retain(|q| q.name != label);
                if self.points.len() == before {
                    return Err(format!("No waypoint \"{label}\""));
                }
                self.save();
                Ok(Some(format!("Deleted \"{label}\"")))
            }
            "clear" => {
                self.points.clear();
                self.save();
                Ok(Some("All waypoints cleared".into()))
            }
            "hud" => match it.next() {
                Some("off") => {
                    self.shown = false;
                    hud::clear();
                    self.last.clear();
                    Ok(Some("Compass hidden".into()))
                }
                Some("on") => {
                    self.shown = true;
                    Ok(Some("Compass shown".into()))
                }
                _ => Err("Usage: /locus hud on|off".into()),
            },
            _ => Err(
                "Commands: tag <name>, go <name>, list, del <name>, clear, hud on|off".into(),
            ),
        }
    }
}

rustcraft_sdk::export_mod!(Locus);

#[cfg(test)]
mod tests {
    use super::*;

    fn near(name: &str, x: f64, y: f64, z: f64) -> Point {
        Point {
            name: name.into(),
            x,
            y,
            z,
        }
    }

    #[test]
    fn encode_decode_round_trip() {
        let points = vec![near("base", 10.0, 64.0, -5.0), near("peak", -3.5, 100.0, 22.25)];
        let bytes = encode(&points);
        let back = decode(&bytes);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "base");
        assert!((back[0].x - 10.0).abs() < 0.01);
        assert!((back[1].z - 22.25).abs() < 0.01);
    }

    #[test]
    fn decode_ignores_garbage_lines() {
        let bytes = b"bad\nbase|1|2|3\nfield|x|y\n".to_vec();
        let points = decode(&bytes);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].name, "base");
    }

    #[test]
    fn nearest_picks_the_closest_horizontally() {
        let state = Locus {
            shown: true,
            points: vec![near("far", 100.0, 64.0, 0.0), near("close", 3.0, 64.0, -4.0)],
            last: Vec::new(),
        };
        let p = Player {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: "minecraft:overworld".into(),
        };
        let (i, d) = state.nearest(&p).unwrap();
        assert_eq!(state.points[i].name, "close");
        assert!((d - 5.0).abs() < 0.001);
    }

    #[test]
    fn compass_line_reports_direction_and_distance() {
        let state = Locus {
            shown: true,
            points: vec![near("east", 10.0, 64.0, 0.0)],
            last: Vec::new(),
        };
        let p = Player {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: "minecraft:overworld".into(),
        };
        assert_eq!(state.compass_line(&p, 0), "east E 10m");
    }
}
