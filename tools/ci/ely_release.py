"""Packages an ElyHorizon client build into the fork's `nightly` release.

The launcher's Nightly channel (Settings -> ElyHorizon builds) reads the
`nightly` pre-release of Sk0vorodka/ElyHorizon: its `horizon-windows-x64.zip`
installs as the version `nightly-<date>-<commit>`, and its SHA256SUMS must be
signed by the fork's Ed25519 key (the public half lives in
crates/horizon-release-sign/release-keys.txt).

    python tools/ci/ely_release.py <horizon-client.exe> <commit> \
        [--key <signing.key>] [--token <github token>] [--repo Sk0vorodka/ElyHorizon]

`--token` defaults to GITHUB_TOKEN. The signing key defaults to
~/.elyhorizon/signing.key (base64 of the 32-byte Ed25519 seed). The release
`nightly` is replaced wholesale: the old one is deleted first, so the
launcher's "latest" is always this one.
"""

import argparse
import base64
import io
import json
import os
import sys
import time
import urllib.error
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

API = "https://api.github.com"
TAG = "nightly"
PLATFORM = "horizon-windows-x64"


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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("client", help="path to the built horizon-client.exe")
    parser.add_argument("commit", help="the commit the client was built from (short)")
    parser.add_argument("--key", default=os.path.expanduser("~/.elyhorizon/signing.key"))
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--repo", default="Sk0vorodka/ElyHorizon")
    args = parser.parse_args()

    client = Path(args.client)
    if not client.is_file() or client.stat().st_size < 1_000_000:
        raise SystemExit(f"{client}: not a horizon-client.exe")
    commit = args.commit.lstrip("^ ")[:7]
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    version = f"nightly-{date}-{commit}"
    zip_name = f"{PLATFORM}.zip"

    # The archive: one top folder, as backend::install_as expects.
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(client, f"{PLATFORM}/horizon-client.exe")
    archive = buf.getvalue()

    # SHA256SUMS in sha256sum's format, signed over "horizon release nightly\n" + the list.
    import hashlib

    digest = hashlib.sha256(archive).hexdigest()
    sums = f"{digest}  {zip_name}\n".encode()
    seed = base64.b64decode(Path(args.key).read_text().strip())
    signature = sign(b"horizon release nightly\n" + sums, seed)

    token = args.token or (Path(args.token).read_text().strip() if Path(args.token).is_file() else "")
    if not token:
        raise SystemExit("no GitHub token: pass --token or set GITHUB_TOKEN")

    # A pre-release tagged `nightly`, replacing the previous one.
    status, _ = api("GET", f"{API}/repos/{args.repo}/releases/tags/{TAG}", token)
    if status == 200:
        status, release = api("GET", f"{API}/repos/{args.repo}/releases/tags/{TAG}", token)
        api("DELETE", f"{API}/repos/{args.repo}/releases/{release['id']}", token)
    status, release = api(
        "POST",
        f"{API}/repos/{args.repo}/releases",
        token,
        payload={
            "tag_name": TAG,
            "target_commitish": "feat/elyby",
            "name": f"Nightly {date} · {commit}",
            "body": (
                f"The fork's own build of the client, commit {commit}: sandboxed mods with the "
                "entities and target capabilities included. The launcher installs it as version "
                f"`{version}` (Settings -> ElyHorizon builds), reusing the resources an installed "
                "version already prepared."
            ),
            "prerelease": True,
        },
    )
    if status not in (200, 201):
        raise SystemExit(f"release: {status} {release}")
    print("release:", release["html_url"])

    for name, blob, kind in [
        (zip_name, archive, "application/zip"),
        ("SHA256SUMS", sums, "text/plain"),
        ("SHA256SUMS.sig", signature.encode(), "text/plain"),
    ]:
        status, uploaded = api(
            "POST",
            f"{API}/repos/{args.repo}/releases/{release['id']}/assets?name={name}",
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
