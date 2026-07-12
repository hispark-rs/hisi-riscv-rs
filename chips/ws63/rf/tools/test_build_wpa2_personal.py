import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("build-wpa2-personal.py")
SPEC = importlib.util.spec_from_file_location("build_wpa2_personal", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CommandRewriteTest(unittest.TestCase):
    def test_define_name_discards_value(self):
        self.assertEqual(MODULE.define_name("-DCONFIG_SAE=1"), "CONFIG_SAE")
        self.assertEqual(MODULE.define_name("-DCONFIG_SAE"), "CONFIG_SAE")
        self.assertIsNone(MODULE.define_name("-Iinclude"))

    def test_replaces_only_output(self):
        command = ["cc", "-o", "old.o", "-c", "source.c"]
        self.assertEqual(
            MODULE.replace_output(command, Path("new.o")),
            ["cc", "-o", "new.o", "-c", "source.c"],
        )

    def test_replaces_only_source(self):
        command = ["cc", "-o", "old.o", "-c", "old.c"]
        self.assertEqual(
            MODULE.replace_source(command, Path("new.c")),
            ["cc", "-o", "old.o", "-c", "new.c"],
        )

    def test_strips_ninja_dependency_outputs(self):
        command = ["cc", "-MD", "-MT", "old-target", "-MF", "old.d", "-c", "source.c"]
        self.assertEqual(
            MODULE.strip_dependency_output(command),
            ["cc", "-c", "source.c"],
        )

    def test_preserved_define_is_not_forbidden(self):
        declared = {"CONFIG_WPA3", "CONFIG_SAE"}
        preserved = {"CONFIG_WPA3"}
        self.assertEqual(declared - preserved, {"CONFIG_SAE"})


if __name__ == "__main__":
    unittest.main()
