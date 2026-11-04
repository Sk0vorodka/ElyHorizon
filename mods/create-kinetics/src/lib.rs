//! Create Kinetics — the rotation simulation for Create for RustCraft.
//!
//! The blocks come from the `create` content mod; this server mod makes them
//! a kinetic network, as Create does: generators (the creative motor, a
//! water wheel, a windmill) drive shafts and cogwheels, large cogwheels
//! halve the speed that passes them, and a millstone needs speed to run. A
//! cogwheel of a network with no power swaps to its idle model
//! (`create:cogwheel_off`), and back when power returns.
//!
//! The mod learns blocks from the players placing and breaking them (the
//! hooks), keeps them across saves in its storage, and re-crawls the
//! networks when the world changes (and, as a safety net, every 2 seconds).
//!
//! Commands:
//! - `/create info` — the network under your feet: its blocks, speed and
//!   what runs.
//! - `/create speed <rpm>` — set the speed of the creative motor you stand
//!   on (1 to 256).
//! - `/create stats` — what the last crawl saw in total.

use rustcraft_sdk::server::{
    world, Args, Argument, BlockChange, BlockPos, Player, Registration, ServerMod,
};
use rustcraft_sdk::storage;
use std::collections::{HashMap, HashSet};

/// A block position in a dimension.
type Pos = (String, i32, i32, i32);

/// The largest network a crawl walks, and the largest set of cogwheels one
/// rescan swaps: past these the crawl stops (the rest keeps its old state
/// until the world changes again).
const MAX_NODES: usize = 512;
const MAX_SWAPS: usize = 4000;

/// What a `create:` block is, and how it joins a network.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Generates its stored speed, joins through every face.
    Motor,
    /// 8 RPM with water beside it, idle without.
    WaterWheel,
    /// 12 RPM high up (y >= 90), 3 low.
    Windmill,
    /// Transmits through every face.
    Cog,
    /// Transmits, and halves the speed that passes it.
    LargeCog,
    /// Transmits along its axis only.
    Shaft,
    /// Transmits through every face.
    Gearbox,
    /// Consumes: runs at 2 RPM or more.
    Millstone,
}

impl Kind {
    /// The kind (and the axis, for shaft-like blocks) of a block state
    /// string, `create:cogwheel[axis=y]`.
    fn of_state(state: &str) -> Option<(Kind, Option<&str>)> {
        let (id, props) = match state.split_once('[') {
            Some((id, props)) => (id, Some(&props[..props.len().saturating_sub(1)])),
            None => (state, None),
        };
        let kind = match id.strip_prefix("create:")? {
            "cogwheel" | "cogwheel_off" => Kind::Cog,
            "large_cogwheel" => Kind::LargeCog,
            "shaft" => Kind::Shaft,
            "gearbox" => Kind::Gearbox,
            "creative_motor" => Kind::Motor,
            "water_wheel" => Kind::WaterWheel,
            "windmill" => Kind::Windmill,
            "millstone" => Kind::Millstone,
            _ => return None,
        };
        let axis = props.and_then(|p| p.strip_prefix("axis="));
        Some((kind, axis))
    }

    fn is_source(self) -> bool {
        matches!(self, Kind::Motor | Kind::WaterWheel | Kind::Windmill)
    }

    /// The word /create info names it by.
    fn name(self) -> &'static str {
        match self {
            Kind::Motor => "motor",
            Kind::WaterWheel => "water wheel",
            Kind::Windmill => "windmill",
            Kind::Cog => "cogwheel",
            Kind::LargeCog => "large cogwheel",
            Kind::Shaft => "shaft",
            Kind::Gearbox => "gearbox",
            Kind::Millstone => "millstone",
        }
    }
}

/// The kind of a state string, when it is ours (no axis needed).
fn kind_of(state: &str) -> Option<Kind> {
    Kind::of_state(state).map(|(k, _)| k)
}

/// Whether a step of `d` crosses a shaft-like block of `axis` (only along
/// the shaft) — used both for leaving and for entering one.
fn crosses_shaft(axis: Option<&str>, d: (i32, i32, i32)) -> bool {
    match axis {
        Some("x") => d.0 != 0,
        Some("z") => d.2 != 0,
        _ => d.1 != 0,
    }
}

/// A network's final state, from the crawl: the speed of every reached
/// block, and what runs.
struct Net {
    /// Every reached block: its position, kind and speed.
    nodes: Vec<(Pos, Kind, u32)>,
    sources: HashMap<&'static str, u32>,
    mills: u32,
    mills_running: u32,
    max_speed: u32,
}

impl Net {
    fn of(seen: &HashMap<Pos, (Kind, u32)>) -> Self {
        let mut net = Net {
            nodes: seen.iter().map(|(p, (k, s))| (p.clone(), *k, *s)).collect(),
            sources: HashMap::new(),
            mills: 0,
            mills_running: 0,
            max_speed: 0,
        };
        for (kind, speed) in seen.values() {
            net.max_speed = net.max_speed.max(*speed);
            if kind.is_source() && *speed > 0 {
                *net.sources.entry(kind.name()).or_default() += 1;
            }
            if *kind == Kind::Millstone {
                net.mills += 1;
                if *speed >= 2 {
                    net.mills_running += 1;
                }
            }
        }
        net
    }
}

struct Kinetics {
    /// Every kinetics block the mod has seen placed, across dimensions.
    blocks: HashMap<Pos, Kind>,
    /// The networks of the last rescan: the blocks of each and their state.
    nets: Vec<(HashSet<Pos>, Net)>,
    /// Cogwheels the last rescan left in the idle model.
    off: HashSet<Pos>,
    dirty: bool,
    spinning: u32,
}

impl Kinetics {
    fn load() -> Self {
        // The kinds are re-read from the world at the first rescan; until
        // then a placeholder keeps the positions remembered.
        let blocks = storage::get("blocks")
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
            .lines()
            .filter_map(parse_pos)
            .map(|p| (p, Kind::Cog))
            .collect::<HashMap<_, _>>();
        Kinetics {
            blocks,
            nets: Vec::new(),
            off: HashSet::new(),
            dirty: true,
            spinning: 0,
        }
    }

    fn save(&self) {
        let text: String = self
            .blocks
            .keys()
            .map(|(d, x, y, z)| format!("{d} {x} {y} {z}\n"))
            .collect();
        storage::set("blocks", text.as_bytes()).ok();
    }

    /// The storage key of a motor's speed (`storage` keys are
    /// `[a-z0-9_.-]`: the dimension's colon becomes an underscore).
    fn motor_key(pos: &Pos) -> String {
        let (d, x, y, z) = pos;
        format!("motor-{}_{}_{}_{}", d.replace(':', "_"), x, y, z)
    }

    /// What a motor here runs at, from its stored speed or the default 16.
    fn motor_speed(&self, pos: &Pos) -> u32 {
        storage::get(&Self::motor_key(pos))
            .and_then(|b| String::from_utf8_lossy(&b).trim().parse().ok())
            .unwrap_or(16)
    }

    /// A generator's RPM: what it makes here and now.
    fn source_speed(&self, pos: &Pos, kind: Kind) -> u32 {
        let (d, x, y, z) = (pos.0.as_str(), pos.1, pos.2, pos.3);
        match kind {
            Kind::Motor => self.motor_speed(pos),
            Kind::WaterWheel => {
                let has_water = [
                    (x + 1, y, z),
                    (x - 1, y, z),
                    (x, y + 1, z),
                    (x, y - 1, z),
                    (x, y, z + 1),
                    (x, y, z - 1),
                ]
                .iter()
                .any(|(nx, ny, nz)| {
                    world::get_block(d, *nx, *ny, *nz)
                        .is_some_and(|s| s.starts_with("minecraft:water"))
                });
                if has_water {
                    8
                } else {
                    0
                }
            }
            Kind::Windmill => {
                if y >= 90 {
                    12
                } else {
                    3
                }
            }
            _ => 0,
        }
    }

    /// Crawls one network from a generator (or from any of its blocks, for
    /// /create info). Speed halves entering a large cogwheel; shafts only
    /// pass along their axis. `seen` gathers the final speed of every block
    /// reached — a caller crawling several generators into one `seen` gets
    /// each block's fastest path.
    fn crawl(
        &self,
        start: &Pos,
        speed: u32,
        seen: &mut HashMap<Pos, (Kind, u32)>,
    ) {
        let mut queue = vec![(start.clone(), speed)];
        while let Some((pos, speed)) = queue.pop() {
            // A stale, slower entry may still sit in the queue.
            if seen.get(&pos).is_some_and(|(_, s)| *s >= speed) {
                continue;
            }
            let (d, x, y, z) = (pos.0.clone(), pos.1, pos.2, pos.3);
            let Some(state) = world::get_block(&d, x, y, z) else {
                continue;
            };
            let Some((kind, axis)) = Kind::of_state(&state) else {
                continue;
            };
            seen.insert(pos.clone(), (kind, speed));
            if seen.len() >= MAX_NODES {
                return;
            }
            let dirs = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
            for dir in dirs {
                // Leaving a shaft: along the shaft.
                if kind == Kind::Shaft && !crosses_shaft(axis, dir) {
                    continue;
                }
                let next = (d.clone(), x + dir.0, y + dir.1, z + dir.2);
                let Some(state) = world::get_block(&next.0, next.1, next.2, next.3) else {
                    continue;
                };
                let Some((next_kind, next_axis)) = Kind::of_state(&state) else {
                    continue;
                };
                // Entering a shaft: along the shaft.
                if next_kind == Kind::Shaft && !crosses_shaft(next_axis, dir) {
                    continue;
                }
                // A large cogwheel halves what passes it.
                let carried = match next_kind {
                    Kind::LargeCog if speed > 1 => speed / 2,
                    _ => speed,
                };
                if seen
                    .get(&next)
                    .is_none_or(|(_, s)| carried > *s)
                {
                    queue.push((next, carried));
                }
            }
        }
    }

    /// Recrawls every generator's network, swaps the cogwheels whose speed
    /// changed, and updates the totals.
    fn rescan(&mut self) {
        // Re-read every tracked block: its kind may have healed from the
        // load's placeholders, and blocks that are no longer ours (or gone)
        // are forgotten.
        let mut fresh: HashMap<Pos, Kind> = HashMap::new();
        for pos in self.blocks.keys() {
            let (d, x, y, z) = (pos.0.as_str(), pos.1, pos.2, pos.3);
            if let Some(kind) = world::get_block(d, x, y, z).and_then(|s| kind_of(&s))
            {
                fresh.insert(pos.clone(), kind);
            }
        }
        self.blocks = fresh;

        // Every generator's crawl: one network per source (a shared block
        // belongs to both), merged into one speed map for the swaps.
        let sources: Vec<(Pos, Kind)> = self
            .blocks
            .iter()
            .filter(|(_, k)| k.is_source())
            .map(|(p, k)| (p.clone(), *k))
            .collect();
        let mut all: HashMap<Pos, (Kind, u32)> = HashMap::new();
        let mut nets: Vec<(HashSet<Pos>, Net)> = Vec::new();
        for (pos, kind) in &sources {
            let speed = self.source_speed(pos, *kind);
            if speed == 0 {
                continue;
            }
            let mut seen: HashMap<Pos, (Kind, u32)> = HashMap::new();
            self.crawl(pos, speed, &mut seen);
            for (p, (k, s)) in &seen {
                if all.get(p).is_none_or(|(_, old)| s > old) {
                    all.insert(p.clone(), (*k, *s));
                }
            }
            nets.push((seen.keys().cloned().collect(), Net::of(&seen)));
        }
        self.nets = nets;

        // Swap cogwheels: a network with speed spins them, no power idles
        // them — including cogwheels no network reaches any more. The axis
        // property carries over.
        let mut changes: HashMap<String, Vec<BlockChange>> = HashMap::new();
        let mut swaps = 0usize;
        for (pos, kind) in &self.blocks {
            if *kind != Kind::Cog {
                continue;
            }
            let speed = all.get(pos).map(|(_, s)| *s).unwrap_or(0);
            let (d, x, y, z) = pos;
            let Some(state) = world::get_block(d, *x, *y, *z) else {
                continue;
            };
            let want = if speed > 0 { "cogwheel" } else { "cogwheel_off" };
            let id = state.split('[').next().unwrap_or(&state);
            if id != format!("create:{want}") && swaps < MAX_SWAPS {
                swaps += 1;
                let props = state
                    .split_once('[')
                    .map(|(_, rest)| format!("[{rest}"))
                    .unwrap_or_default();
                changes.entry(d.clone()).or_default().push(BlockChange {
                    x: *x,
                    y: *y,
                    z: *z,
                    state: format!("create:{want}{props}"),
                });
            }
        }
        for (d, blocks) in changes {
            let _ = world::set_blocks(&d, &blocks);
        }

        self.spinning = all
            .values()
            .filter(|(k, s)| *k == Kind::Cog && *s > 0)
            .count() as u32;
        self.off = self
            .blocks
            .iter()
            .filter(|(p, k)| **k == Kind::Cog && all.get(*p).is_none_or(|(_, s)| *s == 0))
            .map(|(p, _)| p.clone())
            .collect();
    }

    /// The network at (or under) the player's feet, described: the crawl
    /// walks the whole component from here (speed 0), then every generator
    /// it reaches crawls again with its real speed.
    fn describe_at(&self, player: &Player) -> Option<String> {
        let dim = player.dimension.as_str();
        let x = player.x.floor() as i32;
        let y = player.y.floor() as i32;
        let z = player.z.floor() as i32;
        let feet = (dim.to_owned(), x, y, z);
        let below = (dim.to_owned(), x, y - 1, z);
        let here = if self.blocks.contains_key(&feet) {
            feet
        } else if self.blocks.contains_key(&below) {
            below
        } else {
            return None;
        };
        let mut seen: HashMap<Pos, (Kind, u32)> = HashMap::new();
        self.crawl(&here, 0, &mut seen);
        let sources: Vec<Pos> = seen
            .iter()
            .filter(|(_, (k, _))| k.is_source())
            .map(|(p, _)| p.clone())
            .collect();
        for pos in &sources {
            let kind = seen[pos].0;
            let speed = self.source_speed(pos, kind);
            if speed > 0 {
                self.crawl(pos, speed, &mut seen);
            }
        }
        Some(describe(&Net::of(&seen)))
    }
}

/// The words /create info answers with.
fn describe(net: &Net) -> String {
    if net.max_speed == 0 {
        return "No power here: place or repair a generator (motor, water wheel, windmill).".into();
    }
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for (_, kind, _) in &net.nodes {
        *counts.entry(kind.name()).or_default() += 1;
    }
    let mut parts: Vec<String> = counts
        .iter()
        .map(|(name, n)| format!("{n} {name}{}", if *n > 1 { "s" } else { "" }))
        .collect();
    parts.sort();
    let mut out = format!(
        "Network: {} block(s) at {} RPM — {}.",
        net.nodes.len(),
        net.max_speed,
        parts.join(", ")
    );
    if net.mills > 0 {
        out += &format!(" Millstone(s) running: {}/{}.", net.mills_running, net.mills);
    }
    out
}

/// Parses a storage line `dim x y z` into a position.
fn parse_pos(line: &str) -> Option<Pos> {
    let mut it = line.split_whitespace();
    let d = it.next()?.to_owned();
    let x = it.next()?.parse().ok()?;
    let y = it.next()?.parse().ok()?;
    let z = it.next()?.parse().ok()?;
    Some((d, x, y, z))
}

impl ServerMod for Kinetics {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            Kinetics::load(),
            Registration::new()
                .command_with(
                    "create",
                    [
                        Argument::word("sub"),
                        Argument::integer("rpm", Some(1), Some(256)).optional(),
                    ],
                )
                .on_tick()
                .on_blocks(),
        ))
    }

    fn tick(&mut self, tick: u64) {
        if self.dirty || tick % 40 == 0 {
            self.dirty = false;
            self.rescan();
            self.save();
        }
    }

    fn block_placed(&mut self, player: &Player, pos: BlockPos, state: &str) {
        if let Some((kind, _)) = Kind::of_state(state) {
            self.blocks
                .insert((player.dimension.clone(), pos.x, pos.y, pos.z), kind);
            self.dirty = true;
            self.save();
        }
    }

    fn block_broken(&mut self, player: &Player, pos: BlockPos, state: &str) {
        if Kind::of_state(state).is_some() {
            self.blocks
                .remove(&(player.dimension.clone(), pos.x, pos.y, pos.z));
            self.dirty = true;
            self.save();
        }
    }

    fn command(
        &mut self,
        player: &Player,
        name: &str,
        args: &Args,
    ) -> Result<Option<String>, String> {
        debug_assert_eq!(name, "create");
        match args.string("sub") {
            Some("info") => self
                .describe_at(player)
                .ok_or_else(|| "Stand on a cogwheel, shaft or machine of Create.".into())
                .map(Some),
            Some("speed") => {
                let rpm = args.integer("rpm").unwrap_or(16) as u32;
                let dim = &player.dimension;
                let x = player.x.floor() as i32;
                let y = player.y.floor() as i32;
                let z = player.z.floor() as i32;
                for pos in [(dim.clone(), x, y, z), (dim.clone(), x, y - 1, z)] {
                    if self.blocks.get(&pos) == Some(&Kind::Motor) {
                        storage::set(&Self::motor_key(&pos), rpm.to_string().as_bytes())
                            .map_err(|e| format!("Could not store the speed: {e}"))?;
                        self.dirty = true;
                        return Ok(Some(format!("Motor speed set to {rpm} RPM.")));
                    }
                }
                Err("Stand on (or at) a creative motor to set its speed.".into())
            }
            Some("stats") => Ok(Some(format!(
                "Tracked {} block(s); last scan found {} network(s), {} cogwheel(s) spinning.",
                self.blocks.len(),
                self.nets.len(),
                self.spinning
            ))),
            _ => Err("Usage: /create info | /create speed <rpm> | /create stats".into()),
        }
    }
}

rustcraft_sdk::export_server_mod!(Kinetics);

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(state: &str) -> Kind {
        Kind::of_state(state).unwrap().0
    }

    #[test]
    fn states_parse_into_kinds() {
        assert_eq!(kind("create:cogwheel[axis=y]"), Kind::Cog);
        assert_eq!(kind("create:cogwheel_off"), Kind::Cog);
        assert_eq!(kind("create:shaft[axis=x]"), Kind::Shaft);
        assert_eq!(kind("create:creative_motor"), Kind::Motor);
        assert_eq!(Kind::of_state("minecraft:stone"), None, "foreign states are not ours");
        assert_eq!(Kind::of_state("create:cogwheel[axis=y]").unwrap().1, Some("y"));
        assert_eq!(Kind::of_state("create:shaft[axis=x]").unwrap().1, Some("x"));
    }

    #[test]
    fn shafts_only_join_along_their_axis() {
        assert!(crosses_shaft(Some("y"), (0, 1, 0)));
        assert!(!crosses_shaft(Some("y"), (1, 0, 0)));
        assert!(crosses_shaft(Some("x"), (1, 0, 0)));
        assert!(crosses_shaft(Some("z"), (0, 0, -1)));
        assert!(crosses_shaft(None, (0, 1, 0)));
    }

    #[test]
    fn the_info_text_follows_the_network() {
        let mut seen = HashMap::new();
        seen.insert(("o".into(), 0, 0, 0), (Kind::Motor, 16));
        seen.insert(("o".into(), 0, 1, 0), (Kind::Shaft, 16));
        seen.insert(("o".into(), 0, 2, 0), (Kind::Cog, 16));
        seen.insert(("o".into(), 1, 2, 0), (Kind::Millstone, 8));
        let text = describe(&Net::of(&seen));
        assert!(text.contains("4 block(s) at 16 RPM"), "{text}");
        assert!(text.contains("1 cogwheel, 1 millstone, 1 motor, 1 shaft"), "{text}");
        assert!(text.contains("Millstone(s) running: 1/1"), "{text}");
        let dead = describe(&Net::of(&HashMap::new()));
        assert!(dead.starts_with("No power here"), "{dead}");
    }

    #[test]
    fn the_net_counts_mills_and_speeds() {
        let mut seen = HashMap::new();
        seen.insert(("o".into(), 0, 0, 0), (Kind::Motor, 16));
        seen.insert(("o".into(), 0, 1, 0), (Kind::LargeCog, 8));
        seen.insert(("o".into(), 0, 2, 0), (Kind::Cog, 8));
        seen.insert(("o".into(), 0, 3, 0), (Kind::Millstone, 1));
        let net = Net::of(&seen);
        assert_eq!(net.max_speed, 16);
        assert_eq!(net.mills, 1);
        assert_eq!(net.mills_running, 0, "a millstone at 1 RPM cannot run");
        assert_eq!(net.sources["motor"], 1);
    }

    #[test]
    fn positions_round_trip_through_storage() {
        let pos: Pos = ("minecraft:overworld".into(), -12, 64, 300);
        let line = format!("{} {} {} {}", pos.0, pos.1, pos.2, pos.3);
        assert_eq!(parse_pos(&line), Some(pos));
        assert_eq!(parse_pos("one two"), None);
    }
}
