"""Packages an ElyHorizon build into the fork's `nightly` release.

The launcher's Nightly channel (Settings -> ElyHorizon builds) reads the
`nightly` pre-release of Sk0vorodka/ElyHorizon. The release carries the two
downloadables players take:

    ElyHorizonLauncher-x64-<build>.exe   the launcher itself
    ElyHorizonClient-x64-<build>.zip     the version folder, as install_as unpacks it:
                                         horizon-client.exe, horizon-ctl.exe,
                                         horizon-server.exe, research-lock.json,
                                         crates/horizon-prepare (the preparer's sources,
                                         which the launcher checks), tools/,
                                         mods/horizon/src/test/java, LICENSE, PLAY.txt

and the signed SHA256SUMS of both files (the signature is over
`horizon release nightly\\n` + SHA256SUMS, made with the fork's Ed25519 key, the
public half of which lives in crates/horizon-release-sign/release-keys.txt).

    python tools/ci/ely_release.py --build <number or commit> \
        --client <horizon-client.exe> --server <horizon-server.exe> \
        --ctl <horizon-ctl.exe> --launcher <horizon-launcher.exe> \
        [--repo-dir <checkout>] [--key <signing.key>] [--token <github token>]

`--token` defaults to GITHUB_TOKEN. The signing key defaults to
~/.elyhorizon/signing.key (base64 of the 32-byte Ed25519 seed). The release
`nightly` is replaced wholesale: the old one is deleted first, so the
launcher's "latest" is always this one. api.github.com needs HTTPS_PROXY on
networks that gate GitHub.
"""

import argparse
import base64
import hashlib
import io
import json
import os
import shutil
import sys
import time
import urllib.error
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

API = "https://api.github.com"
UPLOADS = "https://uploads.github.com"
TAG = "nightly"
PLATFORM = "horizon-windows-x64"
# What a version folder takes from the checkout besides the programs (xtask's
# ARCHIVED list, PLAY.txt included).
ARCHIVED = [
    "tools",
    "mods/horizon/src/test/java",
    "crates/horizon-prepare",
    "research-lock.json",
    "LICENSE",
]
PLAY = "packaging/PLAY.txt"


def api(method: str, url: str, token: str, payload=None, raw: bytes | None = None, content_type: str = "application/json"):
    data = raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None)
    request = urllib.request.Request(url, data=data, method=method)
    request.add_header("Authorization", f"Bearer {token}")
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("User-Agent", "ely-release")
    if raw is not None:
        request.add_header("Content-Type", content_type)
    last = None
    for attempt in range(5):
        try:
            with urllib.request.urlopen(request) as response:
                body = response.read()
                return response.status, (json.loads(body) if body and content_type == "application/json" else body)
        except urllib.error.HTTPError as e:
            body = e.read()
            if e.code < 500 and e.code != 429:
                return e.code, (json.loads(body) if body and body.strip().startswith(b"{") else body)
            last = e
        except urllib.error.URLError as e:
            last = e
        time.sleep(2 * (attempt + 1))
    raise SystemExit(f"{method} {url}: {last}")


def sign(message: bytes, seed: bytes) -> str:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

    key = Ed25519PrivateKey.from_private_bytes(seed)
    return base64.b64encode(key.sign(message)).decode()


def exe(path: Path) -> Path:
    if not path.is_file() or path.stat().st_size < 100_000:
        raise SystemExit(f"{path}: missing or too small to be the program")
    if path.read_bytes()[:2] != b"MZ":
        raise SystemExit(f"{path}: not a Windows program")
    return path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--build", required=True, help="the name stamp: an AppVeyor build number or a short commit")
    parser.add_argument("--client", required=True, help="horizon-client.exe")
    parser.add_argument("--server", required=True, help="horizon-server.exe")
    parser.add_argument("--ctl", required=True, help="horizon-ctl.exe")
    parser.add_argument("--launcher", required=True, help="horizon-launcher.exe")
    parser.add_argument("--commit", default="", help="the commit built (the release body says it)")
    parser.add_argument("--repo-dir", default=".", help="the ElyHorizon checkout the archived files come from")
    parser.add_argument("--key", default=os.path.expanduser("~/.elyhorizon/signing.key"))
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--repo", default="Sk0vorodka/ElyHorizon")
    args = parser.parse_args()

    repo = Path(args.repo_dir).resolve()
    client = exe(Path(args.client))
    server = exe(Path(args.server))
    ctl = exe(Path(args.ctl))
    launcher = exe(Path(args.launcher))
    stamp = args.build.strip().replace(" ", "-")
    client_zip_name = f"ElyHorizonClient-x64-{stamp}.zip"
    launcher_name = f"ElyHorizonLauncher-x64-{stamp}.exe"
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    version = f"nightly-{date}-{args.commit[:7] if args.commit else stamp}"

    # The version folder: the programs, then what the checkout ships a version
    # with (research-lock.json, the preparer's sources, the tools, LICENSE).
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        top = PLATFORM
        z.write(client, f"{top}/horizon-client.exe")
        z.write(server, f"{top}/horizon-server.exe")
        z.write(ctl, f"{top}/horizon-ctl.exe")
        for archived in ARCHIVED:
            source = repo / archived
            if not source.exists():
                raise SystemExit(f"{source}: the checkout lacks it")
            if source.is_file():
                z.write(source, f"{top}/{archived}")
                continue
            for path in sorted(source.rglob("*")):
                if path.is_file():
                    z.write(path, f"{top}/{archived}/{path.relative_to(source).as_posix()}")
        play = repo / PLAY
        if play.is_file():
            z.write(play, f"{top}/PLAY.txt")
    archive = buf.getvalue()

    # SHA256SUMS over both downloadables, signed for the nightly channel.
    launcher_bytes = launcher.read_bytes()
    sums = "".join(
        f"{hashlib.sha256(blob).hexdigest()}  {name}\n"
        for name, blob in [(client_zip_name, archive), (launcher_name, launcher_bytes)]
    ).encode()
    seed = base64.b64decode(Path(args.key).read_text().strip())
    signature = sign(b"horizon release nightly\n" + sums, seed)

    token = args.token or (Path(args.token).read_text().strip() if Path(args.token).is_file() else "")
    if not token:
        raise SystemExit("no GitHub token: pass --token or set GITHUB_TOKEN")

    commit_line = f"commit {args.commit[:7]} " if args.commit else ""
    status, _ = api("GET", f"{API}/repos/{args.repo}/releases/tags/{TAG}", token)
    if status == 200:
        _, old = api("GET", f"{API}/repos/{args.repo}/releases/tags/{TAG}", token)
        api("DELETE", f"{API}/repos/{args.repo}/releases/{old['id']}", token)
    status, release = api(
        "POST",
        f"{API}/repos/{args.repo}/releases",
        token,
        payload={
            "tag_name": TAG,
            "target_commitish": "feat/elyby",
            "name": f"Nightly {date} · {args.commit[:7] if args.commit else stamp}",
            "body": (
                f"The fork's own build, {commit_line}with sandboxed mods (entities, target "
                "capabilities included). **ElyHorizon builds** in the launcher's settings "
                f"installs the client as version `{version}`, reusing the resources an "
                "installed version already prepared. `ElyHorizonLauncher` plays alone: the "
                "launcher itself, which installs and updates the game from your own copy of "
                "Minecraft."
            ),
            "prerelease": True,
        },
    )
    if status not in (200, 201):
        raise SystemExit(f"release: {status} {release}")
    print("release:", release["html_url"])

    for name, blob, kind in [
        (client_zip_name, archive, "application/zip"),
        (launcher_name, launcher_bytes, "application/octet-stream"),
        ("SHA256SUMS", sums, "text/plain"),
        ("SHA256SUMS.sig", signature.encode(), "text/plain"),
    ]:
        status, uploaded = api(
            "POST",
            f"https://uploads.github.com/repos/{args.repo}/releases/{release['id']}/assets?name={name}",
            token,
            raw=blob,
            content_type=kind,
        )
        if status not in (200, 201):
            raise SystemExit(f"asset {name}: {status} {uploaded}")
        print("uploaded:", name, len(blob), "bytes")

    print("version:", version)


if __name__ == "__main__":
    main()
