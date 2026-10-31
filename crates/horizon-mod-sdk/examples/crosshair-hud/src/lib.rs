//! Example Horizon mod (`docs/mods.md`): what is under your crosshair in
//! the top left corner — the block's name, or the entity's kind, health and
//! custom name — from capability `target`, as Jade-style tooltips show it.
//! `/chud` hides or shows the panel.
use horizon_mod_sdk::{
    Mod, Registration, game,
    hud::{self, Corner, Line},
    target::{self, Crosshair},
};

struct CrosshairHud {
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

/// The panel's lines for what the crosshair is over.
fn lines(look: &Crosshair) -> Vec<String> {
    match look {
        Crosshair::Block(b) => vec![format!("{} · {:.1}m", b.block, b.distance)],
        Crosshair::Entity(e) => {
            let mut lines = vec![format!(
                "{} · {:.1}m",
                e.custom_name
                    .as_deref()
                    .map_or_else(|| kind_name(&e.kind), str::to_owned),
                e.distance
            )];
            if let Some(health) = e.health {
                lines.push(format!("{health:.0} HP"));
            }
            lines
        }
    }
}

impl Mod for CrosshairHud {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            CrosshairHud {
                shown: true,
                last: Vec::new(),
            },
            Registration::new().command("chud").on_frame(),
        ))
    }

    fn frame(&mut self, _seconds: f32) {
        // The crosshair moves with every mouse twitch: the panel follows per
        // frame, while the mod runs (its budget is 0.1 ms).
        let Some(_) = game::get_player().filter(|_| self.shown) else {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        };
        let now = target::look().map(|look| lines(&look)).unwrap_or_default();
        if now != self.last {
            if now.is_empty() {
                hud::clear();
            } else {
                let panel: Vec<Line> = now
                    .iter()
                    .map(|text| Line {
                        text: text.clone(),
                        color: 0xFFFFFF,
                    })
                    .collect();
                hud::set_panel(Corner::TopLeft, &panel);
            }
            self.last = now;
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "chud" {
            return Err("This command does nothing".into());
        }
        self.shown = !self.shown;
        if !self.shown {
            hud::clear();
            self.last.clear();
        }
        let state = if self.shown { "shown" } else { "hidden" };
        Ok(Some(format!("Crosshair panel {state}")))
    }
}

horizon_mod_sdk::export_mod!(CrosshairHud);

#[cfg(test)]
mod tests {
    use super::*;

    fn block(name: &str, distance: f32) -> Crosshair {
        Crosshair::Block(target::BlockTarget {
            block_x: 0,
            block_y: 64,
            block_z: 0,
            block: name.into(),
            distance,
        })
    }

    fn entity(kind: &str, distance: f32, health: Option<f32>) -> Crosshair {
        Crosshair::Entity(target::EntityTarget {
            id: 7,
            kind: kind.into(),
            distance,
            health,
            custom_name: None,
        })
    }

    #[test]
    fn a_block_shows_its_name_and_distance() {
        assert_eq!(
            lines(&block("minecraft:stone", 3.25)),
            vec!["minecraft:stone · 3.2m".to_string()]
        );
    }

    #[test]
    fn an_entity_shows_kind_health_and_distance() {
        assert_eq!(
            lines(&entity("minecraft:pig", 2.0, Some(10.0))),
            vec!["Pig · 2.0m".to_string(), "10 HP".to_string()]
        );
    }

    #[test]
    fn a_custom_name_wins_over_the_kind() {
        assert_eq!(
            lines(&entity("minecraft:pig", 2.0, None)),
            vec!["Pig · 2.0m".to_string()]
        );
        let mut named = entity("minecraft:pig", 2.0, None);
        if let Crosshair::Entity(e) = &mut named {
            e.custom_name = Some("Bob".into());
        }
        assert_eq!(lines(&named), vec!["Bob · 2.0m".to_string()]);
    }
}
