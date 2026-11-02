"""Verify real Git checkout bytes under each autocrlf policy; no live repository writes."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS = Path("crates/rroflow-ingest/migrations")


class MigrationCheckoutTests(unittest.TestCase):
    def checkout(self, policy, attributes):
        with tempfile.TemporaryDirectory(prefix="rroflow-migration-checkout-") as temporary:
            repo = Path(temporary) / "repo"
            repo.mkdir()
            def git(*args):
                return subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True)
            git("init", "-q")
            if attributes:
                (repo / ".gitattributes").write_bytes((ROOT / ".gitattributes").read_bytes())
            (repo / MIGRATIONS).mkdir(parents=True)
            expected = {}
            for source in (ROOT / MIGRATIONS).glob("*.sql"):
                expected[source.name] = source.read_bytes()
                self.assertFalse(b"\r" in expected[source.name], f"Non-LF migration: {source.name}")
                (repo / MIGRATIONS / source.name).write_bytes(expected[source.name])
            self.assertTrue(expected)
            git("-c", "core.autocrlf=false", "add", ".")
            output = Path(temporary) / "checkout"
            git("-c", f"core.autocrlf={policy}", "-c", "core.eol=crlf",
                "checkout-index", "--all", "--force", f"--prefix={output.as_posix()}/")
            return expected, {name: (output / MIGRATIONS / name).read_bytes() for name in expected}

    def test_migration_bytes_survive_all_checkout_policies(self):
        for policy in ("true", "false", "input"):
            with self.subTest(autocrlf=policy):
                expected, actual = self.checkout(policy, attributes=True)
                self.assertEqual(expected, actual)

    def test_unprotected_checkout_reproduces_the_checksum_difference(self):
        expected, actual = self.checkout("true", attributes=False)
        self.assertNotEqual(expected, actual)
        self.assertTrue(any(b"\r\n" in value for value in actual.values()))


if __name__ == "__main__":
    unittest.main()
