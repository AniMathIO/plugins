#!/usr/bin/env python3
"""Offline tests for scripts/registry.py: python3 -m unittest discover -s scripts (needs jsonschema)."""
import copy
import io
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import registry  # noqa: E402

REAL = json.loads(registry.REGISTRY.read_text())
EXTERNAL = {"id": "com.example.spiral", "name": "Spiral", "author": "Example", "repo": "example/spiral",
            "description": "A spiral clip source for tests", "license": "Apache-2.0 OR MIT", "min_abi": "1.0.0", "repo_id": 4242}


def amplugin(pid="com.example.spiral", version="1.2.0", abi="1.0.0", kinds=("clip-source",)):
    manifest = (f'api_version = "{abi}"\nid = "{pid}"\nname = "Spiral"\nversion = "{version}"\n'
                f'kinds = {json.dumps(list(kinds))}\n')
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as z:
        z.writestr("plugin.toml", manifest)
        z.writestr("plugin.wasm", b"\0asm")
    return buffer.getvalue()


class FakeSource:
    """Stands in for GitHub: releases = {tag: {asset_name: bytes}}."""

    def __init__(self, releases, prerelease=()):
        self.releases_data, self.prerelease = releases, set(prerelease)

    def releases(self, repo):
        return [{"tag_name": tag, "draft": False, "prerelease": tag in self.prerelease,
                 "published_at": "2026-01-01T00:00:00Z",
                 "assets": [{"name": n, "browser_download_url": f"https://github.com/{repo}/releases/download/{tag}/{n}"}
                            for n in assets]} for tag, assets in self.releases_data.items()]

    def fetch(self, asset, limit):
        tag = asset["browser_download_url"].split("/download/")[1].split("/")[0]
        return self.releases_data[tag][asset["name"]]

    repo_id = 4242

    def api(self, path):
        return {"id": self.repo_id, "private": False, "archived": False, "license": {"spdx_id": "MIT"}}


def release(version, data=None, digest=None, **kw):
    data = data or amplugin(version=version, **kw)
    name = f"com.example.spiral-{version}.amplugin"
    digest = digest or registry.sha256_hex(data)
    return {name: data, name + ".sha256": f"{digest}  {name}\n".encode()}


class RegistryFile:
    def __init__(self, test, registry_data):
        self.directory = tempfile.TemporaryDirectory()
        test.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "community-plugins.json"
        self.path.write_text(json.dumps(registry_data))
        patcher = mock.patch.object(registry, "REGISTRY", self.path)
        patcher.start()
        test.addCleanup(patcher.stop)


class ValidateTests(unittest.TestCase):
    def check(self, mutate, expect=None):
        data = copy.deepcopy(REAL)
        mutate(data)
        RegistryFile(self, data)
        errors = registry.validate(offline=True)
        if expect is None:
            self.assertEqual(errors, [])
        else:
            self.assertTrue(any(expect in e for e in errors), errors)

    def test_committed_registry_is_valid(self):
        self.check(lambda d: None)

    def test_readme_table_in_sync(self):
        self.assertTrue(registry.sync_readme(write=False))

    def test_external_entry_is_valid(self):
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL)))

    def test_reserved_id(self):
        def mutate(d):
            d["plugins"].append(dict(EXTERNAL, id="org.animathio.evil"))
        self.check(mutate, "reserved")

    def test_windows_device_ids_are_rejected(self):
        for stem in ("con", "prn", "aux", "nul", "com1", "com9", "lpt1", "lpt9"):
            with self.subTest(stem=stem):
                self.check(lambda d: d["plugins"].append(dict(EXTERNAL, id=f"{stem}.example.plugin")),
                           "Windows device names")
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, id="com0.example.plugin")))

    def test_duplicate_id(self):
        self.check(lambda d: d["plugins"].append(dict(d["plugins"][0], name="Other")), "duplicate id")

    def test_duplicate_name(self):
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, name="code snippets")), "already used")

    def test_id_must_be_reverse_domain(self):
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, id="spiral")), "does not match")

    def test_external_entry_needs_repo_id_and_it_must_match(self):
        self.check(lambda d: d["plugins"].append({k: v for k, v in EXTERNAL.items() if k != "repo_id"}), "need repo_id")
        data = copy.deepcopy(REAL)
        data["plugins"].append(dict(EXTERNAL))
        RegistryFile(self, data)
        self.assertEqual(registry.validate(offline=False, source=FakeSource({})), [])
        other = FakeSource({})
        other.repo_id = 7
        self.assertTrue(any("repo_id says" in e for e in registry.validate(offline=False, source=other)))

    def test_license_must_be_osi_approved(self):
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, license="Proprietary")), "not OSI-approved")
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, license="CC-BY-NC-4.0")), "not OSI-approved")

    def test_unknown_field_rejected(self):
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, version="1.0.0")), "Additional properties")

    def test_hosted_needs_path_and_matching_manifest(self):
        self.check(lambda d: d["plugins"][0].pop("path"), "need a path")
        self.check(lambda d: d["plugins"][0].update(min_abi="1.0.0"), "min_abi 1.0.0 differs")
        self.check(lambda d: d["plugins"][0].update(id="io.github.animathio.other"), "has id")

    def test_delisted_ids_cannot_return(self):
        def mutate(d):
            d["delisted"].append({"id": d["plugins"][0]["id"], "reason": "removed for tests", "date": "2026-11-17"})
        self.check(mutate, "delisted")

    def test_path_traversal_rejected(self):
        self.check(lambda d: d["plugins"].append(dict(EXTERNAL, path="../x")), "")

    def test_license_ids(self):
        self.assertEqual(registry.license_ids("MIT AND OFL-1.1"), ["MIT", "OFL-1.1"])
        self.assertEqual(registry.license_ids("(Apache-2.0 OR MIT) AND GPL-3.0-or-later WITH Classpath-exception-2.0"),
                         ["Apache-2.0", "MIT", "GPL-3.0-or-later"])


class IndexTests(unittest.TestCase):
    def build(self, releases, strict=True, previous=None, prerelease=(), entry=None):
        data = copy.deepcopy(REAL)
        data["plugins"] = [dict(entry or EXTERNAL)]
        RegistryFile(self, data)
        return registry.build_index(FakeSource(releases, prerelease), previous or {}, strict, False)

    def test_picks_highest_published_release(self):
        index, problems = self.build({"v1.2.0": release("1.2.0"), "v1.10.0": release("1.10.0"),
                                      "v2.0.0": release("2.0.0"), "v1.9.0": release("1.9.0")},
                                     prerelease=["v2.0.0"])
        self.assertEqual(problems, [])
        entry = index["plugins"][0]
        self.assertEqual((entry["version"], entry["min_abi"], entry["kinds"]), ("1.10.0", "1.0.0", ["clip-source"]))
        self.assertEqual(entry["sha256"], registry.sha256_hex(amplugin(version="1.10.0")))
        self.assertEqual(entry["download_url"],
                         "https://github.com/example/spiral/releases/download/v1.10.0/com.example.spiral-1.10.0.amplugin")
        self.assertEqual(registry.schema_errors(index, "community-index.schema.json"), [])

    def test_monorepo_entries_only_accept_id_tags(self):
        mono = dict(EXTERNAL, path="plugins/spiral")
        _, problems = self.build({"v1.2.0": release("1.2.0")}, entry=mono)
        self.assertTrue(problems and "no published release" in problems[0])
        index, problems = self.build({"com.example.spiral@1.2.0": release("1.2.0")}, entry=mono)
        self.assertEqual((problems, index["plugins"][0]["path"]), ([], "plugins/spiral"))

    def test_repository_identity_is_pinned(self):
        source = FakeSource({"v1.2.0": release("1.2.0")})
        source.repo_id = 7  # a different repository now occupies the configured name
        data = copy.deepcopy(REAL)
        data["plugins"] = [dict(EXTERNAL)]
        RegistryFile(self, data)
        index, problems = registry.build_index(source, {}, True, False)
        self.assertIn("now has GitHub id 7", problems[0])
        self.assertEqual(index["plugins"], [])

    def test_checksum_mismatch(self):
        _, problems = self.build({"v1.2.0": release("1.2.0", digest="0" * 64)})
        self.assertIn("does not match", problems[0])

    def test_manifest_must_match_registry_and_tag(self):
        _, problems = self.build({"v1.2.0": release("1.2.0", data=amplugin(pid="com.example.other"))})
        self.assertIn("expected 'com.example.spiral'", problems[0])
        _, problems = self.build({"v1.2.0": release("1.2.0", data=amplugin(version="1.1.0"))})
        self.assertIn("expected '1.2.0'", problems[0])
        _, problems = self.build({"v1.2.0": release("1.2.0", abi="1.1.0")})
        self.assertIn("min_abi", problems[0])

    def test_missing_assets_and_bad_archive(self):
        _, problems = self.build({"v1.2.0": {}})
        self.assertIn("lacks asset", problems[0])
        _, problems = self.build({"v1.2.0": release("1.2.0", data=b"not a zip")})
        self.assertIn("not a zip", problems[0])

    def test_non_strict_keeps_previous_entry_only_for_transient_errors(self):
        good, _ = self.build({"v1.2.0": release("1.2.0")})

        class Flaky(FakeSource):
            def releases(self, repo):
                raise registry.TransientError("GitHub is down")

        data = copy.deepcopy(REAL)
        data["plugins"] = [dict(EXTERNAL)]
        RegistryFile(self, data)
        index, problems = registry.build_index(Flaky({}), good, False, False)
        self.assertTrue(problems)
        self.assertEqual(index["plugins"][0]["version"], "1.2.0")
        # A release that is gone or does not verify is never served from the old index.
        for releases in ({}, {"v1.3.0": release("1.3.0", digest="0" * 64)}):
            index, problems = self.build(releases, strict=False, previous=good)
            self.assertTrue(problems)
            self.assertEqual(index["plugins"], [])


if __name__ == "__main__":
    unittest.main()
