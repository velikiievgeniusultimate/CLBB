# SPDX-License-Identifier: GPL-3.0-or-later
"""No runtime input: user-frozen hero requests are rejected before delegation."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("frozen_hero_guard", Path(__file__).resolve().parents[2] / "tools/visit-unconfigured-heroes.py")
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class FrozenHeroGuardTests(unittest.TestCase):
    def test_mixed_request_cannot_visit_a_frozen_hero(self):
        with self.assertRaisesRegex(ValueError, "frozen heroes:.*25"):
            guard.validate_request([1, 25, 2], {"heroes": [{"id": 25}]})

    def test_empty_or_malformed_manifest_cannot_disable_protection(self):
        for manifest in ({}, None, {"heroes": []}, {"heroes": [{"id": True}]},
                         {"heroes": [{"id": "25"}]}, {"heroes": [{"id": 25}, {"id": 25}]}):
            with self.subTest(manifest=manifest), self.assertRaises(ValueError):
                guard.validate_request([1], manifest)

    def test_new_heroes_remain_available(self):
        self.assertEqual(guard.validate_request([1, 2, 3], {"heroes": [{"id": 25}, {"id": 84}]}), [25, 84])


if __name__ == "__main__":
    unittest.main()
