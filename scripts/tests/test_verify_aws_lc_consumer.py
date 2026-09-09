"""Checks for the paired consumer's active dependency audit."""

import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_aws_lc_consumer import (
    check_consumer_dependencies,
    consumer_dependencies,
)

SOURCE = "registry+https://example.test/index"


def fixture(candidate=False, cc_version="1.4.4"):
    generator = "toucan_bindgen" if candidate else "bindgen"
    versions = {
        "consumer": "0.0.0",
        "aws-lc-sys": "0.44.0",
        "cc": cc_version,
        generator: "0.0.1" if candidate else "0.72.1",
    }
    identities = [(name, version) for name, version in versions.items()]
    identities.extend([("shlex", "1.3.0"), ("shlex", "2.0.1")])
    packages = [
        {
            "id": f"{name}@{version}",
            "name": name,
            "version": version,
            "source": None if name == "consumer" else SOURCE,
        }
        for name, version in identities
    ]
    nodes = {p["id"]: {"id": p["id"], "features": [], "deps": []} for p in packages}

    def edge(parent, name, target, kind=None):
        nodes[parent]["deps"].append(
            {"name": name, "pkg": target, "dep_kinds": [{"kind": kind, "target": None}]}
        )

    edge("consumer@0.0.0", "aws_lc_sys", "aws-lc-sys@0.44.0")
    edge("aws-lc-sys@0.44.0", "cc", f"cc@{cc_version}", "build")
    edge("aws-lc-sys@0.44.0", "bindgen", f"{generator}@{versions[generator]}", "build")
    edge(f"cc@{cc_version}", "shlex", "shlex@2.0.1")
    edge(
        f"{generator}@{versions[generator]}",
        "shlex",
        "shlex@2.0.1" if candidate else "shlex@1.3.0",
    )
    tree = "\n".join(
        f"{name} v{version}|"
        for name, version in identities
        if not (candidate and name == "shlex" and version == "1.3.0")
    )
    lock = {
        "package": [
            {
                "name": p["name"],
                "version": p["version"],
                "source": p["source"],
                "checksum": p["id"],
            }
            for p in packages
        ]
    }
    metadata = {
        "packages": packages,
        "resolve": {"root": "consumer@0.0.0", "nodes": list(nodes.values())},
    }
    return metadata, tree, lock, generator


def graph(args):
    return consumer_dependencies(*args)


class ConsumerDependencies(unittest.TestCase):
    def test_generator_only_version_can_disappear_while_shared_version_is_preserved(
        self,
    ):
        before, after = graph(fixture()), graph(fixture(True))
        check_consumer_dependencies(before, after)
        self.assertIn("shlex@2.0.1", after["packages"])
        self.assertNotIn("shlex@1.3.0", before["packages"])
        self.assertEqual(len(after["edges"]), 3)

    def test_native_edge_cannot_switch_version_while_old_version_remains_active(self):
        args = list(fixture(True))
        args[1] += "\nshlex v1.3.0|"
        cc = next(n for n in args[0]["resolve"]["nodes"] if n["id"] == "cc@1.4.4")
        cc["deps"][0]["pkg"] = "shlex@1.3.0"
        # shlex2 remains active through the replacement generator, so comparing
        # package names or the global version set cannot establish this edge.
        with self.assertRaisesRegex(RuntimeError, "non-generator"):
            check_consumer_dependencies(graph(fixture()), graph(args))

    def test_native_package_version_cannot_drift(self):
        with self.assertRaisesRegex(RuntimeError, "non-generator"):
            check_consumer_dependencies(graph(fixture()), graph(fixture(True, "1.4.5")))

    def test_shared_checksum_source_and_features_cannot_drift(self):
        for change in ("checksum", "source", "features"):
            args = copy.deepcopy(fixture(True))
            package = next(p for p in args[0]["packages"] if p["id"] == "shlex@2.0.1")
            locked = next(
                p
                for p in args[2]["package"]
                if p["name"] == "shlex" and p["version"] == "2.0.1"
            )
            if change == "features":
                next(
                    n for n in args[0]["resolve"]["nodes"] if n["id"] == package["id"]
                )["features"].append("extra")
            elif change == "source":
                package["source"] = locked["source"] = (
                    "registry+https://different.test/index"
                )
            else:
                locked["checksum"] = "changed"
            with (
                self.subTest(change=change),
                self.assertRaisesRegex(RuntimeError, "non-generator"),
            ):
                check_consumer_dependencies(graph(fixture()), graph(args))

    def test_only_the_expected_generator_build_edge_is_removed(self):
        args = list(fixture(True))
        args[3] = "bindgen"
        with self.assertRaisesRegex(RuntimeError, "generator edge"):
            graph(args)
        args = list(fixture(True))
        sys = next(
            n for n in args[0]["resolve"]["nodes"] if n["id"] == "aws-lc-sys@0.44.0"
        )
        next(d for d in sys["deps"] if d["name"] == "bindgen")["dep_kinds"][0][
            "kind"
        ] = None
        with self.assertRaisesRegex(RuntimeError, "generator edge"):
            graph(args)


if __name__ == "__main__":
    unittest.main()
