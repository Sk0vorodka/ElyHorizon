//! Example Horizon mod (`docs/mods.md`): the entities around you in the top
//! right corner — how many, of what kinds, and the nearest one by name —
//! from capability `entities`, refreshed twice a second. `/radar` hides or
//! shows the panel.
use horizon_mod_sdk::{
    Mod, Registration, entities, game,
    hud::{self, Corner, Line},
};
use std::collections::BTreeMap;

struct EntityRadar {
    shown: bool,
    /// The panel last sent, so an unchanged one costs no call.
    last: Vec<String>,
}

/// `Pig` from `minecraft:pig`; `Zombie Horse` from `minecraft:zombie_horse`.
fn kind_name(kind: &str) -> String {
    kind.rsplit(':')
        .next()
        .unwrap_or(kind)
        .split('_')
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How far an entity's feet are from the player's, in blocks.
fn distance(player: &game::Player, x: f64, y: f64, z: f64) -> f64 {
    ((x - player.x).powi(2) + (y - player.y).powi(2) + (z - player.z).powi(2)).sqrt()
}

/// The panel's lines: the census by kind (at most five kinds), then the
/// nearest entity with its distance.
fn summary(player: &game::Player, near: &[entities::Entity]) -> Vec<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for e in near {
        *counts.entry(e.kind.as_str()).or_default() += 1;
    }
    let mut lines = vec![format!("{} entities nearby", near.len())];
    for (kind, count) in counts.iter().take(5) {
        lines.push(format!("{} × {}", count, kind_name(kind)));
    }
    if let Some(e) = near.iter().min_by(|a, b| {
        let da = distance(player, a.x, a.y, a.z);
        let db = distance(player, b.x, b.y, b.z);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Greater)
    }) {
        lines.push(format!(
            "Nearest: {} · {:.0}m",
            kind_name(&e.kind),
            distance(player, e.x, e.y, e.z)
        ));
    }
    lines
}

impl Mod for EntityRadar {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            EntityRadar {
                shown: true,
                last: Vec::new(),
            },
            Registration::new().command("radar").on_tick(),
        ))
    }

    fn tick(&mut self, tick: u64) {
        // A census twice a second is plenty for a panel.
        if tick % 10 != 0 {
            return;
        }
        let Some(player) = game::get_player().filter(|_| self.shown) else {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        };
        let Some(near) = entities::list(32.0) else {
            return;
        };
        let now = summary(&player, &near);
        if now != self.last {
            let panel: Vec<Line> = now
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::TopRight, &panel);
            self.last = now;
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "radar" {
            return Err("This command does nothing".into());
        }
        self.shown = !self.shown;
        if !self.shown {
            hud::clear();
            self.last.clear();
        }
        let state = if self.shown { "shown" } else { "hidden" };
        Ok(Some(format!("Entity radar {state}")))
    }
}

horizon_mod_sdk::export_mod!(EntityRadar);

#[cfg(test)]
mod tests {
    use super::*;

    fn player_at(x: f64, y: f64, z: f64) -> game::Player {
        game::Player {
            x,
            y,
            z,
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: "minecraft:overworld".into(),
        }
    }

    fn entity(id: u32, kind: &str, x: f64, y: f64, z: f64) -> entities::Entity {
        entities::Entity {
            id,
            kind: kind.into(),
            x,
            y,
            z,
            yaw: 0.0,
            health: Some(10.0),
            custom_name: None,
        }
    }

    #[test]
    fn kinds_read_like_names() {
        assert_eq!(kind_name("minecraft:pig"), "Pig");
        assert_eq!(kind_name("minecraft:zombie_horse"), "Zombie Horse");
        assert_eq!(kind_name("pig"), "Pig");
    }

    #[test]
    fn the_summary_counts_kinds_and_names_the_nearest() {
        let player = player_at(0.0, 64.0, 0.0);
        let near = vec![
            entity(1, "minecraft:pig", 3.0, 64.0, 0.0),
            entity(2, "minecraft:pig", 10.0, 64.0, 0.0),
            entity(3, "minecraft:cow", 1.0, 64.0, 0.0),
        ];
        let lines = summary(&player, &near);
        assert_eq!(lines[0], "3 entities nearby");
        assert_eq!(lines[1], "1 × Cow");
        assert_eq!(lines[2], "2 × Pig");
        assert_eq!(lines[3], "Nearest: Cow · 1m");
    }

    #[test]
    fn an_empty_area_names_no_nearest() {
        let player = player_at(0.0, 64.0, 0.0);
        let lines = summary(&player, &[]);
        assert_eq!(lines, vec!["0 entities nearby".to_string()]);
    }
}
