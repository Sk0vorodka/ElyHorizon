"""Builds the Create content mod (mods/create/) from the local Create jar.

Takes Create 6.x's plain Blockbench models, blockstates and textures (they
live in the same `create:` namespace our mod id uses, so most files copy
verbatim), writes our block/item declarations, recipes, loot, tags and lang,
and bakes a 4-frame spinning cogwheel texture.

    python tools/mods/create_assets.py --jar <create jar> [--repo .]

Create is made by simibubi and the Create team (assets CC BY-NC-SA 4.0);
this keeps the attribution in the mod's lang and manifest.
"""

import argparse
import json
import os
import zipfile
from pathlib import Path

# create: namespace texture/model refs resolve inside the jar the same way
# they resolve in our mod's content tree.
JAR_ASSETS = "assets/create/"
TEXTURES = "assets/create/textures/"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--jar",
        default=r"C:\Users\artem\AppData\Roaming\16Launcher\game\instances"
        r"\1.21.1(2)\minecraft\mods\create-1.21.1-6.0.10.jar",
    )
    parser.add_argument("--repo", default=".")
    args = parser.parse_args()
    repo = Path(args.repo)
    out = repo / "mods" / "create"
    jar = zipfile.ZipFile(args.jar)
    names = set(jar.namelist())

    def jar_text(name: str) -> str:
        return jar.read(name).decode()

    def write(rel: str, data) -> None:
        path = out / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(data, str):
            data = data.encode()
        path.write_bytes(data)

    copied = set()

    def copy_texture(ref: str) -> None:
        """Copies one `create:` texture ref (and the mcmeta beside it). The
        jar path is the mod path: assets/create/textures/..."""
        if not ref.startswith("create:"):
            return
        png = TEXTURES + ref[len("create:"):] + ".png"
        if png in names and png not in copied:
            copied.add(png)
            write(png, jar.read(png))
            meta = png + ".mcmeta"
            if meta in names:
                write(meta, jar.read(meta))

    def fix_model(model: dict) -> dict:
        """Inlines every `create:` texture ref, and replaces parents that do
        not exist in the jar (Create generates large_wheels in code) with the
        standard block parent so the item view still has transforms."""
        for ref in (model.get("textures") or {}).values():
            if isinstance(ref, str):
                copy_texture(ref.removeprefix("#"))
        parent = model.get("parent", "")
        if parent.startswith("create:block/large_wheels"):
            model["parent"] = "minecraft:block/block"
        if parent.startswith("create:"):
            parent_path = JAR_ASSETS + "models/" + parent[len("create:"):] + ".json"
            if parent_path in names:
                fix_model(json.loads(jar_text(parent_path)))
            else:
                model["parent"] = "minecraft:block/block"
        return model

    def take_model(ref: str) -> dict:
        """Copies a create: model with its texture closure; returns it so the
        caller can tweak and re-write it (a model two blocks share is
        written twice, which is fine)."""
        model = json.loads(
            jar_text(JAR_ASSETS + "models/" + ref[len("create:"):] + ".json")
        )
        fix_model(model)
        rel = "assets/create/models/" + ref[len("create:"):] + ".json"
        write(rel, json.dumps(model, indent="\t"))
        return model

    def axis_blockstate(model: str, x=90, y=90) -> dict:
        """The variants of an axis-state block (the oak_log template), one
        model rotated per axis — Create's own axis rotations."""
        return {
            "variants": {
                "axis=y": {"model": model},
                "axis=x": {"model": model, "x": x, "y": y},
                "axis=z": {"model": model, "x": x, "y": 180},
            }
        }

    # ---- shaft (verbatim) -------------------------------------------------
    write("assets/create/blockstates/shaft.json", jar_text(JAR_ASSETS + "blockstates/shaft.json"))
    take_model("create:block/shaft")
    copy_texture("create:block/axis")
    copy_texture("create:block/axis_top")

    # ---- cogwheel: the crafted, always-spinning wheel; and cogwheel_off ---
    take_model("create:block/cogwheel")
    take_model("create:block/large_cogwheel")
    # The spin model: the cogwheel's, on the animated 4-frame texture.
    spin = json.loads(jar_text(JAR_ASSETS + "models/block/cogwheel.json"))
    fix_model(spin)
    spin["textures"]["1_2"] = "create:block/cogwheel_spin"
    write(
        "assets/create/models/block/cogwheel_spin.json",
        json.dumps(spin, indent="\t"),
    )
    write(
        "assets/create/blockstates/cogwheel.json",
        json.dumps(axis_blockstate("create:block/cogwheel_spin"), indent="\t"),
    )
    write(
        "assets/create/blockstates/cogwheel_off.json",
        json.dumps(axis_blockstate("create:block/cogwheel"), indent="\t"),
    )
    write(
        "assets/create/blockstates/large_cogwheel.json",
        json.dumps(axis_blockstate("create:block/large_cogwheel"), indent="\t"),
    )

    # The spin: the cogwheel's texture rotated in 90-degree steps (the teeth
    # line up again each step), 2 ticks per frame. The off texture is frame 0.
    from PIL import Image

    cog = Image.open(__import__("io").BytesIO(jar.read(TEXTURES + "block/cogwheel.png"))).convert("RGBA")
    frames = [cog.rotate(-90 * i, expand=False) for i in range(4)]
    sheet = Image.new("RGBA", (cog.width, cog.height * len(frames)))
    for i, frame in enumerate(frames):
        sheet.paste(frame, (0, i * cog.height))
    buf = __import__("io").BytesIO()
    sheet.save(buf, "PNG")
    write("assets/create/textures/block/cogwheel_spin.png", buf.getvalue())
    write(
        "assets/create/textures/block/cogwheel_spin.png.mcmeta",
        json.dumps({"animation": {"frames": [0, 1, 2, 3], "frametime": 2}}, indent="\t"),
    )

    # ---- gearbox (verbatim blockstate + model) ----------------------------
    write("assets/create/blockstates/gearbox.json", jar_text(JAR_ASSETS + "blockstates/gearbox.json"))
    take_model("create:block/gearbox/block")
    take_model("create:block/gearbox/item")

    # ---- millstone (verbatim: no states, one model) ------------------------
    write("assets/create/blockstates/millstone.json", jar_text(JAR_ASSETS + "blockstates/millstone.json"))
    take_model("create:block/millstone/block")
    take_model("create:block/millstone/item")

    # ---- creative motor: Create's blockstate uses facing, ours has axis ---
    take_model("create:block/creative_motor/block")
    take_model("create:block/creative_motor/block_vertical")
    take_model("create:block/creative_motor/item")
    write(
        "assets/create/blockstates/creative_motor.json",
        json.dumps(
            {
                "variants": {
                    "axis=y": {"model": "create:block/creative_motor/block_vertical"},
                    "axis=x": {"model": "create:block/creative_motor/block", "x": 90, "y": 90},
                    "axis=z": {"model": "create:block/creative_motor/block", "x": 90, "y": 180},
                }
            },
            indent="\t",
        ),
    )

    # ---- water wheel: the wheel's plain JSON model, on axis states --------
    take_model("create:block/water_wheel/block")
    write(
        "assets/create/blockstates/water_wheel.json",
        json.dumps(axis_blockstate("create:block/water_wheel/block"), indent="\t"),
    )

    # ---- windmill: our own hub — an axis pillar crossed by two wool bars --
    write(
        "assets/create/blockstates/windmill.json",
        json.dumps(axis_blockstate("create:block/windmill"), indent="\t"),
    )
    write(
        "assets/create/models/block/windmill.json",
        json.dumps(
            {
                "parent": "minecraft:block/block",
                "textures": {
                    "axis": "create:block/axis",
                    "axis_top": "create:block/axis_top",
                    "sail": "minecraft:block/white_wool",
                    "particle": "create:block/axis",
                },
                "elements": [
                    {
                        "name": "axis",
                        "from": [6, 0, 6],
                        "to": [10, 16, 10],
                        "faces": {
                            "north": {"uv": [6, 0, 10, 16], "texture": "#axis"},
                            "east": {"uv": [6, 0, 10, 16], "texture": "#axis"},
                            "south": {"uv": [6, 0, 10, 16], "texture": "#axis"},
                            "west": {"uv": [6, 0, 10, 16], "texture": "#axis"},
                            "up": {"uv": [6, 6, 10, 10], "texture": "#axis_top"},
                            "down": {"uv": [6, 6, 10, 10], "texture": "#axis_top"},
                        },
                    },
                    {
                        "name": "sail_east_west",
                        "from": [0, 6, 6],
                        "to": [16, 10, 10],
                        "faces": {
                            "north": {"uv": [0, 0, 16, 4], "texture": "#sail"},
                            "south": {"uv": [0, 0, 16, 4], "texture": "#sail"},
                            "up": {"uv": [0, 0, 16, 4], "texture": "#sail"},
                            "down": {"uv": [0, 0, 16, 4], "texture": "#sail"},
                        },
                    },
                    {
                        "name": "sail_north_south",
                        "from": [6, 6, 0],
                        "to": [10, 10, 16],
                        "faces": {
                            "east": {"uv": [0, 0, 16, 4], "texture": "#sail"},
                            "west": {"uv": [0, 0, 16, 4], "texture": "#sail"},
                            "up": {"uv": [0, 0, 4, 16], "texture": "#sail"},
                            "down": {"uv": [0, 0, 4, 16], "texture": "#sail"},
                        },
                    },
                ],
            },
            indent="\t",
        ),
    )

    # ---- item model defs (26.3 items/ format) ------------------------------
    for item, model in [
        ("cogwheel", "create:block/cogwheel_spin"),
        ("cogwheel_off", "create:block/cogwheel"),
        ("large_cogwheel", "create:block/large_cogwheel"),
        ("shaft", "create:block/shaft"),
        ("gearbox", "create:block/gearbox/item"),
        ("creative_motor", "create:block/creative_motor/item"),
        ("water_wheel", "create:block/water_wheel/block"),
        ("windmill", "create:block/windmill"),
        ("millstone", "create:block/millstone/item"),
    ]:
        write(
            f"assets/create/items/{item}.json",
            json.dumps({"model": {"type": "minecraft:model", "model": model}}, indent="\t"),
        )

    # ---- blocks ------------------------------------------------------------
    def block(name: str, template: str, **props) -> None:
        doc = {"template": template, "item": True}
        if props:
            doc["properties"] = props
        write(f"data/create/horizon/block/{name}.json", json.dumps(doc, indent="\t"))

    wood = {"hardness": 0.8, "sounds": "minecraft:oak_log"}
    metal = {"hardness": 3.0, "sounds": "minecraft:iron_block", "requires_tool": True}
    block("cogwheel", "minecraft:oak_log", **wood)
    block("cogwheel_off", "minecraft:oak_log", **wood)
    block("large_cogwheel", "minecraft:oak_log", **wood)
    block("shaft", "minecraft:oak_log", hardness=0.6, sounds="minecraft:oak_log")
    block("gearbox", "minecraft:oak_log", hardness=1.0, sounds="minecraft:iron_block")
    block("creative_motor", "minecraft:oak_log", **metal)
    block("water_wheel", "minecraft:oak_log", hardness=0.8, sounds="minecraft:oak_log")
    block("windmill", "minecraft:oak_log", hardness=0.8, sounds="minecraft:oak_log")
    block("millstone", "minecraft:smooth_stone", hardness=2.0, sounds="minecraft:grindstone")

    # ---- loot: every block drops itself, the off wheel drops the real one --
    for name in [
        "cogwheel", "large_cogwheel", "shaft", "gearbox", "creative_motor",
        "water_wheel", "windmill", "millstone",
    ]:
        write(
            f"data/create/loot_table/blocks/{name}.json",
            json.dumps(
                {
                    "type": "minecraft:block",
                    "pools": [
                        {
                            "rolls": 1,
                            "entries": [{"type": "minecraft:item", "name": f"create:{name}"}],
                            "conditions": [{"condition": "minecraft:survives_explosion"}],
                        }
                    ],
                    "random_sequence": f"create:blocks/{name}",
                },
                indent="\t",
            ),
        )
    write(
        "data/create/loot_table/blocks/cogwheel_off.json",
        json.dumps(
            {
                "type": "minecraft:block",
                "pools": [
                    {
                        "rolls": 1,
                        "entries": [{"type": "minecraft:item", "name": "create:cogwheel"}],
                        "conditions": [{"condition": "minecraft:survives_explosion"}],
                    }
                ],
                "random_sequence": "create:blocks/cogwheel",
            },
            indent="\t",
        ),
    )

    # ---- tags ---------------------------------------------------------------
    axe = ["cogwheel", "large_cogwheel", "shaft", "water_wheel", "windmill", "cogwheel_off"]
    write(
        "data/minecraft/tags/block/mineable/axe.json",
        json.dumps({"values": [f"create:{n}" for n in axe]}, indent="\t"),
    )
    write(
        "data/minecraft/tags/block/mineable/pickaxe.json",
        json.dumps(
            {"values": ["create:gearbox", "create:creative_motor", "create:millstone"]},
            indent="\t",
        ),
    )

    # ---- recipes (26.3 data-pack crafting) ----------------------------------
    def shaped(name: str, pattern: str, key: dict, result: str, count: int = 1) -> None:
        write(
            f"data/create/recipe/{name}.json",
            json.dumps(
                {
                    "type": "minecraft:crafting_shaped",
                    "category": "misc",
                    "pattern": list(pattern.split("/")),
                    "key": key,
                    "result": {"id": result, "count": count},
                },
                indent="\t",
            ),
        )

    shaped("shaft", "I/I", {"I": "minecraft:iron_ingot"}, "create:shaft", 4)
    shaped(
        "cogwheel", " P /PSP/ P ",
        {"P": "minecraft:oak_planks", "S": "create:shaft"},
        "create:cogwheel", 2,
    )
    shaped(
        "large_cogwheel", "PPP/PSP/PPP",
        {"P": "minecraft:oak_planks", "S": "create:shaft"},
        "create:large_cogwheel",
    )
    shaped(
        "gearbox", "C C/CGC/C C",
        {"C": "minecraft:cobblestone", "G": "create:cogwheel"},
        "create:gearbox",
    )
    shaped(
        "creative_motor", "IRI/ICI/IRI",
        {"I": "minecraft:iron_ingot", "R": "minecraft:redstone", "C": "create:cogwheel"},
        "create:creative_motor",
    )
    shaped(
        "water_wheel", "PPP/PLP/PPP",
        {"P": "minecraft:oak_planks", "L": "create:large_cogwheel"},
        "create:water_wheel",
    )
    shaped(
        "windmill", "WWW/WSW/WWW",
        {"W": "minecraft:white_wool", "S": "create:shaft"},
        "create:windmill",
    )
    shaped(
        "millstone", "SSS/SCS/SSS",
        {"S": "minecraft:stone", "C": "create:cogwheel"},
        "create:millstone",
    )

    # ---- lang ---------------------------------------------------------------
    names_en = {
        "cogwheel": "Cogwheel",
        "large_cogwheel": "Large Cogwheel",
        "shaft": "Shaft",
        "gearbox": "Gearbox",
        "creative_motor": "Creative Motor",
        "water_wheel": "Water Wheel",
        "windmill": "Windmill",
        "millstone": "Millstone",
    }
    names_ru = {
        "cogwheel": "Шестерня",
        "large_cogwheel": "Большая шестерня",
        "shaft": "Вал",
        "gearbox": "Коробка передач",
        "creative_motor": "Творческий мотор",
        "water_wheel": "Водяное колесо",
        "windmill": "Ветряное колесо",
        "millstone": "Мельница",
    }
    for lang, names in [("en_us", names_en), ("ru_ru", names_ru)]:
        write(
            f"assets/create/lang/{lang}.json",
            json.dumps(
                {
                    **{f"block.create.{k}": v for k, v in names.items()},
                    "itemGroup.create": "Create (RustCraft)",
                    "create.credit": "Block textures and models by the Create team (simibubi and contributors), CC BY-NC-SA 4.0",
                },
                ensure_ascii=False,
                indent="\t",
            ),
        )

    # ---- manifest ------------------------------------------------------------
    write(
        "horizon-mod.toml",
        """\
# Create for RustCraft, the content half (API 0.2): the kinetic blocks with
# Create's own textures and models. The simulation lives in the
# create-kinetics server mod. Create is by simibubi and the Create team
# (assets CC BY-NC-SA 4.0) - this mod carries their art with attribution.
id = "create"
name = "Create for RustCraft"
version = "0.1.0"
description = "The base of Create on RustCraft: cogwheels, shafts, a gearbox, generators (creative motor, water wheel, windmill) and a millstone, with Create's own textures. Pair with the create-kinetics mod for the rotation simulation."
authors = ["Sk0vorodka", "Create team (textures and models)"]
api = "0.2"
side = "both"
capabilities = ["registries"]
""",
    )
    print(f"mods/create: {len(copied)} textures, {sum(1 for _ in out.rglob('*.json'))} json files")


if __name__ == "__main__":
    main()
