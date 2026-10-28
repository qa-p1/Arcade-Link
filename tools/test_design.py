"""Design kit checks: generated files current, format helpers agree (TS, QML JS)."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CASES = json.loads((ROOT / "design/format-cases.json").read_text(encoding="utf-8"))

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
