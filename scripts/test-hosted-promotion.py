import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("promotion", Path(__file__).with_name("promote-hosted-artifact.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
COMMIT = "a" * 40
NAME = f"rroflow-ingest-{COMMIT}-x86_64-unknown-linux-gnu.tar.gz"


def fixture(change=None, extra=None, link=False):
    info = dict(revision=COMMIT, target="x86_64-unknown-linux-gnu", migrations="b" * 64)
    files = {"rroflow-ingest": b"fixture binary", "revision": (COMMIT + "\n").encode(),
             "target": (info["target"] + "\n").encode(), "migrations": (info["migrations"] + "\n").encode(),
             "build-info.json": json.dumps(info).encode()}
    if change:
        change(files)
    files["SHA256SUMS"] = "".join(f"{hashlib.sha256(data).hexdigest()}  {name}\n" for name, data in sorted(files.items())).encode()
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as tar:
        for name, data in files.items():
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            if link and name == "rroflow-ingest":
                entry.type = tarfile.SYMTYPE
                entry.linkname = "/untrusted"
                entry.size = 0
            tar.addfile(entry, io.BytesIO(data))
    contents = archive.getvalue()
    zipped = io.BytesIO()
    with zipfile.ZipFile(zipped, "w") as package:
        package.writestr(NAME, contents)
        package.writestr(NAME + ".sha256", hashlib.sha256(contents).hexdigest() + "\n")
        if extra:
            package.writestr(extra, "unexpected")
    return zipped.getvalue()


def verify(data):
    return module.verified_files(data, COMMIT, "sha256:" + hashlib.sha256(data).hexdigest())


class PromotionTests(unittest.TestCase):
    def test_exact_assets_are_preserved(self):
        data = fixture()
        files = verify(data)
        with zipfile.ZipFile(io.BytesIO(data)) as package:
            self.assertEqual(files[NAME], package.read(NAME))

    def test_changed_ci_zip_digest_is_refused(self):
        with self.assertRaisesRegex(ValueError, "CI artifact digest"):
            module.verified_files(fixture(), COMMIT, "sha256:" + "0" * 64)

    def test_extra_path_or_member_is_refused(self):
        for extra in ["../outside", "/outside", "extra"]:
            with self.assertRaisesRegex(ValueError, "membership"):
                verify(fixture(extra=extra))

    def test_symlink_is_not_a_server_binary(self):
        with self.assertRaisesRegex(ValueError, "membership or type"):
            verify(fixture(link=True))

    def test_recomputed_checksums_cannot_hide_wrong_identity(self):
        for field in ["revision", "target", "migrations"]:
            with self.assertRaisesRegex(ValueError, "identity"):
                verify(fixture(change=lambda files: files.__setitem__(field, b"wrong")))

    def test_decompression_has_an_explicit_bound(self):
        original = module.LIMIT
        try:
            module.LIMIT = 4096
            with self.assertRaisesRegex(ValueError, "Expanded server archive exceeds bounds"):
                verify(fixture())
        finally:
            module.LIMIT = original

    def test_inner_archive_corruption_is_refused(self):
        data = fixture()
        output = io.BytesIO()
        with zipfile.ZipFile(io.BytesIO(data)) as original, zipfile.ZipFile(output, "w") as changed:
            changed.writestr(NAME, original.read(NAME) + b"changed")
            changed.writestr(NAME + ".sha256", original.read(NAME + ".sha256"))
        with self.assertRaisesRegex(ValueError, "Server archive digest"):
            verify(output.getvalue())


if __name__ == "__main__":
    unittest.main()
