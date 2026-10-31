"""Verify a qualified CI zip and its server archive before writing release assets."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import sys
import tarfile
import zipfile

LIMIT = 256 * 1024 * 1024


def verified_files(data, commit, expected_digest):
    if not re.fullmatch(r"[a-f0-9]{40}", commit):
        raise ValueError("Invalid commit")
    if len(data) > LIMIT or "sha256:" + hashlib.sha256(data).hexdigest() != expected_digest:
        raise ValueError("CI artifact digest mismatch")
    name = f"rroflow-ingest-{commit}-x86_64-unknown-linux-gnu.tar.gz"
    with zipfile.ZipFile(io.BytesIO(data)) as package:
        entries = package.infolist()
        if sorted(e.filename for e in entries) != sorted([name, name + ".sha256"]):
            raise ValueError("Unexpected CI archive membership")
        if sum(e.file_size for e in entries) > LIMIT or any(e.flag_bits & 1 for e in entries):
            raise ValueError("CI archive exceeds bounds or is encrypted")
        archive = package.read(name)
        checksum = package.read(name + ".sha256")
    if checksum.decode("ascii").strip() != hashlib.sha256(archive).hexdigest():
        raise ValueError("Server archive digest mismatch")
    names = ["SHA256SUMS", "build-info.json", "migrations", "revision", "rroflow-ingest", "target"]
    with gzip.GzipFile(fileobj=io.BytesIO(archive)) as compressed:
        tar_bytes = compressed.read(LIMIT + 1)
    if len(tar_bytes) > LIMIT:
        raise ValueError("Expanded server archive exceeds bounds")
    files = {}
    total = 0
    with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as server:
        for member in server:
            if member.name not in names or member.name in files or not member.isfile():
                raise ValueError("Unexpected server archive membership or type")
            total += member.size
            if total > LIMIT:
                raise ValueError("Server archive exceeds bounds")
            files[member.name] = server.extractfile(member).read()
    if sorted(files) != names:
        raise ValueError("Unexpected server archive membership")
    expected = {n: hashlib.sha256(files[n]).hexdigest() for n in names if n != "SHA256SUMS"}
    observed = {}
    for line in files["SHA256SUMS"].decode("ascii").splitlines():
        match = re.fullmatch(r"([a-f0-9]{64})  ([a-zA-Z0-9.\-]+)", line)
        if not match or match[2] in observed:
            raise ValueError("Invalid member checksums")
        observed[match[2]] = match[1]
    if observed != expected:
        raise ValueError("Server member digest mismatch")
    info = json.loads(files["build-info.json"])
    if (info.get("revision") != commit or files["revision"].decode().strip() != commit
            or info.get("target") != "x86_64-unknown-linux-gnu"
            or files["target"].decode().strip() != info["target"]
            or not re.fullmatch(r"[a-f0-9]{64}", info.get("migrations", ""))
            or files["migrations"].decode().strip() != info["migrations"]):
        raise ValueError("Server identity mismatch")
    return {name: archive, name + ".sha256": checksum}


if __name__ == "__main__":
    source, commit, expected_digest, destination = sys.argv[1:]
    if Path(source).stat().st_size > LIMIT:
        raise ValueError("CI archive exceeds bounds")
    assets = verified_files(Path(source).read_bytes(), commit, expected_digest)
    output = Path(destination)
    if any((output / name).exists() for name in assets):
        raise ValueError("Release assets already exist; refusing replacement")
    output.mkdir(parents=True, exist_ok=True)
    for name, contents in assets.items():
        with (output / name).open("xb") as target:
            target.write(contents)
