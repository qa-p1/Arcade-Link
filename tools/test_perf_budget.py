import json
import os
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "fleet"))
import perf_budget  # noqa: E402


class PerfBudget(unittest.TestCase):
    def test_a_result_over_a_limit_or_missing_fails(self):
        limits = {"cold_start_ms": 1500, "open_cpu_pct": 2.0, "disk_mb": 400}
        self.assertEqual(perf_budget.over_budget("x", {"cold_start_ms": 900, "open_cpu_pct": 2.0, "disk_mb": 1}, limits), [])
        failures = perf_budget.over_budget("x", {"cold_start_ms": 1501, "open_cpu_pct": 0.1}, limits)
        self.assertEqual(failures, ["x: cold_start_ms 1501 > budget 1500", "x: disk_mb was not measured"])

    @unittest.skipUnless(sys.platform.startswith("linux"), "perf_budget reads /proc (Linux only)")
    def test_counters_read_this_process(self):
        ticks, switches = perf_budget.counters(os.getpid())
        self.assertGreaterEqual(ticks, 0)
        self.assertGreater(switches, 0)
        self.assertIsNone(perf_budget.counters(2**22 + 12345))

    def test_every_perf_budget_names_a_metric_the_tool_reports(self):
        metrics = {"cold_start_ms", "show_ms", "disk_mb"} | {f"{s}_{m}" for s in ("background", "open", "closed")
                                                            for m in ("cpu_pct", "wakeups")}
        budgets = json.loads((Path(__file__).resolve().parent / "fleet" / "budgets.json").read_text())
        for section, data in budgets.items():
            for app, limits in (data.get("perf", {}) if isinstance(data, dict) else {}).items():
                if app.startswith("_"):
                    continue
                self.assertLessEqual({k for k in limits if not k.startswith("_")}, metrics, f"{section}/{app}")


if __name__ == "__main__":
    unittest.main()
