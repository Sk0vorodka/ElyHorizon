<p align="center">
  <img src="docs/media/banner.webp" alt="ElyHorizon: Horizon — Minecraft: Java Edition 26.3 rewritten in Rust — with ely.by accounts" width="100%">
</p>

<p align="center">
  <a href="https://ci.appveyor.com/project/Sk0vorodka/elyhorizon"><img src="https://ci.appveyor.com/api/projects/status/Sk0vorodka/elyhorizon/branch/feat/elyby?svg=true" alt="Build"></a>
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F" alt="Minecraft Java 26.3">
  <img src="https://img.shields.io/badge/Rust-stable-B7410E?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/accounts-ely.by-3C8CFF" alt="ely.by accounts">
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-2A2470" alt="Linux, macOS and Windows">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-3B2F7E" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#download"><b>Download</b></a> ·
  <a href="#what-this-fork-adds"><b>ely.by</b></a> ·
  <a href="#making-a-mod"><b>Mods</b></a> ·
  <a href="https://playhorizon.dev/docs/"><b>Documentation</b></a> ·
  <a href="#project-status"><b>Project status</b></a> ·
  <a href="#credits"><b>Credits</b></a>
</p>

# ElyHorizon

ElyHorizon is a fork of [Horizon](https://github.com/horizon-rust/horizon) — Minecraft: Java Edition 26.3 rewritten in Rust — that plays with **[ely.by](https://ely.by) accounts**: no Microsoft account needed. You sign in to the launcher with your ely.by profile (password and two-factor code, or OAuth in the browser), the game joins servers with it, and your ely.by skin shows in the world.

Everything Horizon is, this fork is too: a native game that reads the same worlds, follows the same rules and generates the same terrain as the Java game, checked against it by 281 differential harnesses. Free, open source and unofficial.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft. ElyHorizon contains no Minecraft code, assets or data: you need your own legally obtained copy of Minecraft: Java Edition 26.3. ely.by is an independent account service, not affiliated with Mojang.

## What this fork adds

- **ely.by sign-in, two ways.** In the launcher's account form: your ely.by e-mail and password (two-factor accounts ask for the 6-digit code), or **Sign in with browser** — an OAuth page at account.ely.by, with the launcher catching the callback locally.
- **Your ely.by skin in the game.** The launcher carries the skin through the session file, so the client draws it without reaching ely.by at play time; accounts signed in before the skin fetch worked heal themselves on the next launch.
- **Online play as an ely.by player.** Online-mode servers that accept ely.by sessions (authlib-injector servers, Horizon's own server) and every offline-mode server, including Open to LAN with a friend on the Java game.
- **Everything upstream:** the launcher, the mod system, the server, the agent control, the parity work. See the sections below.

The ely.by sign-in and the skins live in the launcher (`launcher/src/account/`): the Yggdrasil authenticate flow against `authserver.ely.by` and the OAuth2 flow against `account.ely.by`, with the skin fetched from the authserver's profile API.

## Download

Windows builds of both binaries are made on [AppVeyor](https://ci.appveyor.com/project/Sk0vorodka/elyhorizon) for every push to `feat/elyby`: open the latest build's **Artifacts** tab and take `horizon-launcher.exe` and `horizon-client.exe`. The launcher installs and updates the game, prepares its files from your own copy of Minecraft, and signs you in.

**You need** an [ely.by](https://ely.by) account (make one at [account.ely.by/register](https://account.ely.by/register)) or a Microsoft account that owns Minecraft: Java Edition; Minecraft: Java Edition 26.3, played once (the official launcher, Prism, MultiMC and other launchers all work); a Java 25 runtime, for the one-time preparation; and a GPU with Vulkan.

## Making a mod

Horizon's mod system (Rust compiled to sandboxed WebAssembly — Java mod jars do not run) carries over untouched, and this fork builds on it. A mod is a `cdylib` crate against [the SDK](crates/horizon-mod-sdk):

```rust
use horizon_mod_sdk::{Mod, Registration, entities, game, hud::{self, Corner, Line}};

struct Radar;
impl Mod for Radar {
    fn init() -> Result<(Self, Registration), String> {
        Ok((Radar, Registration::new().on_tick()))
    }
    fn tick(&mut self, _tick: u64) {
        let Some(_player) = game::get_player() else { return };
        if let Some(near) = entities::list(32.0) {
            hud::set_panel(Corner::TopRight, &[Line {
                text: format!("{} entities nearby", near.len()),
                color: 0xFFFFFF,
            }]);
        }
    }
}
horizon_mod_sdk::export_mod!(Radar);
```

The SDK's capabilities cover HUD panels and shapes, a minimap, zoom, mod-owned screens with widgets, the entities and the crosshair target around the player, recipes with a recipe-viewer panel, input and storage. Worked examples live in [crates/horizon-mod-sdk/examples](crates/horizon-mod-sdk/examples) (`entity-radar`, `crosshair-hud`, `coords-hud` and more); the contract is [`crates/horizon-mod-sdk/wit/horizon.wit`](crates/horizon-mod-sdk/wit/horizon.wit). See [Mods and data packs](https://playhorizon.dev/docs/mods).

## What works today

The [documentation](https://playhorizon.dev/docs/) covers each of these.

- **The game:** singleplayer with vanilla's world generation, mobs, villagers, raids, redstone, every command, the HUD and the recipe book, on saves in vanilla's format. Worlds from 1.8 to 26.2 are upgraded as vanilla does. [Status table](#project-status).
- **Horizon's own extras:** new menus (vanilla's are one setting away), graphics presets, distant terrain up to 1024 chunks, controller support, optional zoom, minimap and more, all off by default where they would change the vanilla look. See [Settings](https://playhorizon.dev/docs/settings).
- **Resource packs and data packs:** resource packs work, and data packs load functions, predicates and item modifiers.
- **Playing with others:** invite friends from the pause menu's Friends button with a code, Open to LAN, and joining servers — online-mode ones with the ely.by or Microsoft account the launcher signs in with. See [Multiplayer](https://playhorizon.dev/docs/multiplayer).

## Host a server

The launcher's **Servers** page creates, starts, backs up and updates a Horizon server with buttons, and invites friends over Tailscale. On a VPS, `horizon-server` installs with one command and runs without Java. Guide: [Host a server](https://playhorizon.dev/docs/hosting) ([reference](docs/hosting.md)).

## Build from source

For developers. Requirements: Rust 1.90 or newer, Python 3.10 or newer, JDK 25, and on Linux the ALSA and udev development headers (`libasound2-dev libudev-dev` on Debian and Ubuntu). Development and the vanilla harnesses run on Linux.

Horizon contains no game files and never downloads them. It builds its resources from **your own copy** of Minecraft: Java Edition 26.3, verified against the checksums in `research-lock.json`:

1. **Client:** install Java Edition with the official launcher and play 26.3 once. Horizon reads `versions/26.3/`, `libraries/` and `assets/` from the launcher's game directory (`~/.minecraft` on Linux). If yours lives elsewhere, pass `--minecraft-dir` or set `HORIZON_MINECRAFT_DIR`.
2. **Server jar:** the 26.3 server jar, from the `downloads.server.url` in your launcher's `versions/26.3/26.3.json` (or [minecraft.net/download/server](https://www.minecraft.net/en-us/download/server) while 26.3 is the current release). Pass it with `--server-jar` or set `HORIZON_SERVER_JAR`. It is only needed the first time.

```sh
git clone https://github.com/Sk0vorodka/ElyHorizon.git
cd ElyHorizon
cargo run --release -p horizon-prepare -- --prepare --server-jar ~/Downloads/server.jar   # needs JDK 25
cargo run --release -p horizon-prepare -- sounds   # optional: sounds from your launcher install
cargo run --release -p horizon-client
```

Everything prepared lands in the gitignored `.local/`. It is derived from the game: never commit it or share it.

The client starts on the title screen. Command-line options (`--world`, `--seed`, `--creative`, `--render-distance`, `--dedicated`), controls, scripting through `horizon-ctl` and the architecture are described in [the native client guide](docs/native-client.md). The rest of the developer documentation is indexed in [docs/README.md](docs/README.md), including [the parity workflow](docs/parity-workflow.md).

## Project status

The table below is the upstream project's own audit of each subsystem. Every subsystem now matches vanilla in every harness that compares it. 100% means 100% of what the harnesses compare, not proof that nothing else differs.

### Java parity: 100%

Each subsystem scores 0 (absent), 0.25 (started, not playable), 0.5 (playable, major vanilla features missing), 0.75 (playable, mostly complete) or 1 (complete and verified against vanilla by differential tests). The percentage is the average; `tests/test_parity.py` keeps it in sync with this table. Last audited 2026-10-13 against the harness results: all 281 harnesses pass on Linux, each rerun whenever one of its inputs changed, and the Mac mini now runs every one of them too (Rust uses Java's own float maths on each platform).

| Subsystem | Score | Main gaps |
| --- | ---: | --- |
| Window, input and game loop | 1 | None: the game loop, options, camera, auto-jump, toggle sprint/sneak, local input and the frame limiter match vanilla |
| Assets and resources | 1 | None: item models match vanilla over 196,290 stacks and the client draws every item through them; the resource pack screen matches vanilla's drawing over 3,000 sessions of clicks, scrolling and searching, its incompatibility confirmation included |
| Network protocol and multiplayer | 1 | None: joins vanilla servers; the dedicated server (`--dedicated`) matches vanilla's files, status ping, login, configuration, play, chunks, chat, recipes, advancements and command tree, a client built from vanilla's packet code joins it, Horizon joins a vanilla server, and LAN discovery matches over 462 cases (offline mode only) |
| Chunk storage and palettes | 1 | None: client and saves share paletted containers, identical to vanilla over 266 scenarios |
| Section meshing | 1 | None: 112 scenes and over a million quads identical to vanilla |
| Rendering | 1 | None: entity models, block entity renderers (407 cases, End portals, gateways and the loom's banner preview included) and their animations (296 cases, End gateway beams included), the HUD overlays (2,200 states), the recipe book (600 states), the player list, subtitles and the world border match vanilla |
| Lighting | 1 | None: saved and relit light matches vanilla, slabs and stairs included |
| World generation | 1 | None: 2,352 whole chunks over 225 structure sites, in all three dimensions, identical to vanilla |
| Region files and saves | 1 | None: saves round-trip with vanilla, and worlds from 1.8 to 26.2 upgrade as vanilla's DataFixers do, superflat worlds included; old options.txt and hotbar.nbt files are upgraded too |
| Entities and physics | 1 | None: attributes, dimensions, death loot, spawn rules, natural spawning (15,500 cases), raids and patrols (1,900 cases), spawned mobs' gear, enchantments and state (`finalizeSpawn`, 24,000 cases) and mob AI (92 scenarios: villager breeding and gossip, the warden, ghastlings and copper golems included) match vanilla |
| Interaction and inventory | 1 | None: menu clicks match over 8,000 click sequences, the loom and the crafter included; recipe book placement (6,000 cases), its buttons (1,200), the saved book (1,500) and the loom and crafter screens (1,600) match vanilla |
| Menus, HUD and chat | 1 | None: every command runs as vanilla (758 commands over 39,685 parsed inputs), scoreboards and teams match (81,000 operations), the player list matches over 872,000 draw calls with heads drawn from players' skins (authlib's textures, SkinManager's lookup, legacy skin conversion), and the chat matches focused and scrolled; the resource pack screen (selection over 4,000 cases, drawing and its incompatibility confirmation over 3,000 sessions) and the chat input with its command suggestions, usage lines, Tab completion and history (3,000 sessions on vanilla's client command tree) match vanilla |
| Audio | 1 | None: the sound engine, sound events, music, situational music, ambience and subtitles match vanilla |
| Block behaviours and ticking | 1 | None: block updates, random ticks, redstone, dispensers (6,000 cases), crafters (4,000 crafts), block entity removal (containers spill, 4,000 cases), copper golem chest sorting and the sculk family (435 scenarios: shrieker warnings and warden summoning included) match vanilla |

### Known issues

Skins served over https show the default skin unless already cached; pack icons show the unknown-pack texture in game; the creative hotbar keys (C/X) aren't wired to the hotbar loader; item tooltips lack attribute lines.

## Licence

[MIT](LICENSE), for the project's original work only. Dependencies keep their own licences. Minecraft is a trademark of Mojang Synergies AB; Minecraft code and assets belong to Mojang and Microsoft and are covered by the [Minecraft EULA](https://www.minecraft.net/en-us/eula). Decompiled game source and prepared resources stay on your machine and are never published.

Third-party credits and licenses: [Third-party notices](THIRD_PARTY_NOTICES.md).

## Credits

ElyHorizon is a fork of [Horizon](https://github.com/horizon-rust/horizon) by [Ludovic Marie](https://lu4ovic.com) ([@ludovic111](https://github.com/ludovic111)) — the engine, the launcher and the mod system are the upstream project's work; this fork's own work is the ely.by integration, its documentation and its builds. Accounts: [ely.by](https://ely.by), an independent Minecraft account service.
