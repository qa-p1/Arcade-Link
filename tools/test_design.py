"""Design kit checks: generated files current, format helpers agree (TS, QML JS)."""
import json
import copy
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CASES = json.loads((ROOT / "design/format-cases.json").read_text(encoding="utf-8"))
sys.path.insert(0, str(ROOT / "design"))
import generate

RUNNER = """
const cases = JSON.parse(require('fs').readFileSync(process.argv[3], 'utf8'));
const f = require(process.argv[2]);
const out = [];
for (const [n, want] of cases.size) out.push(['size', n, f.formatSize(n), want]);
for (const [n, want] of cases.count) out.push(['count', n, f.formatCount(n), want]);
for (const [n, style, want] of cases.duration) out.push(['duration', n, f.formatDuration(n, style), want]);
for (const [t, now, offset, want] of cases.relative) out.push(['relative', t, f.formatRelative(t, now, offset), want]);
console.log(JSON.stringify(out));
"""


class DesignTests(unittest.TestCase):
    def test_small_text_and_badges_pass_aa_on_every_component_surface(self):
        tokens = json.loads((ROOT / "design/tokens.json").read_text())
        for theme in ("light", "dark"):
            neutral = tokens["neutral"][theme]
            surfaces = [neutral[name] for name in ("bg", "panel", "sunken", "hover")]
            pairs = [(neutral[name], surfaces) for name in ("text", "muted")]
            for name, ink in generate.status_inks(tokens, theme).items():
                pairs.append((ink, surfaces + [tokens["status"][theme][f"{name}-soft"]]))
            for app, accent in tokens["accent"].items():
                pairs.append((generate.accent_ink(tokens, app, theme), surfaces + [accent["soft"][theme]]))
            for foreground, backgrounds in pairs:
                for background in backgrounds:
                    with self.subTest(theme=theme, foreground=foreground, background=background):
                        self.assertGreaterEqual(generate.contrast(foreground, background), 4.5)

    def test_validation_rejects_low_contrast_metadata(self):
        tokens = json.loads((ROOT / "design/tokens.json").read_text())
        shortcuts = json.loads((ROOT / "design/shortcuts.json").read_text())
        broken = copy.deepcopy(tokens)
        broken["neutral"]["light"]["muted"] = tokens["neutral"]["light"]["faint"]
        errors, _ = generate.validate(broken, shortcuts, "")
        self.assertTrue(any("muted on bg" in error for error in errors), errors)

    def test_components_use_readable_foregrounds(self):
        css = (ROOT / "design/css/components.css").read_text()
        for name in ("ok", "warn", "danger", "accent"):
            self.assertIn(f".badge.{name} {{ background: var(--{name}-soft); color: var(--{name}-ink);", css)
        self.assertIn(".key-or { color: var(--muted);", css)
        self.assertNotIn("color: var(--faint); font-size:", css)
        dart = (ROOT / "design/dart/arcade_widgets.dart").read_text()
        for name in ("ok", "warn", "danger", "accent"):
            self.assertIn(f"ArcadeTone.{name} => (fg: c.{name}Ink,", dart)

    def test_generated_files_are_current(self):
        result = subprocess.run([sys.executable, str(ROOT / "design/generate.py"), "--check"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def run_js(self, source: str):
        node = shutil.which("node")
        if not node:
            self.skipTest("node is not installed")
        with tempfile.TemporaryDirectory() as tmp:
            module = Path(tmp) / "format.cjs"
            module.write_text(source + "\nmodule.exports = { formatSize, formatCount, formatDuration, formatRelative };\n", encoding="utf-8")
            runner = Path(tmp) / "run.cjs"
            runner.write_text(RUNNER, encoding="utf-8")
            result = subprocess.run([node, str(runner), str(module), str(ROOT / "design/format-cases.json")], capture_output=True, text=True, check=True)
        for kind, given, got, want in json.loads(result.stdout):
            self.assertEqual(got, want, f"{kind}({given})")

    def test_qml_format_matches_the_cases(self):
        source = (ROOT / "design/qml/Arcade/Format.js").read_text(encoding="utf-8").replace(".pragma library", "")
        self.run_js(source)

    def test_typescript_format_matches_the_cases(self):
        # format.ts uses only erasable annotations: strip them the same way.
        sys.path.insert(0, str(ROOT / "design"))
        import generate
        source = generate.emit_qml_format((ROOT / "design/css/format.ts").read_text(encoding="utf-8")).replace(".pragma library", "")
        self.run_js(source)


if __name__ == "__main__":
    unittest.main()
