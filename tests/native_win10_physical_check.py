"""Verify physical Windows host evidence from `nexus-engine physical-check`.

Reads the `native-win10-physical.json` evidence file written by the engine and
validates the evidence contract. The command always writes the JSON (even when
WHP is unavailable, e.g. on a nested-virtualization CI VM), so this verifier
supports two modes:

  default             structural check only: schema + host_os identity. Passes
                      even if WHP is absent (CI-safe).
  --require-win10     host must be a Windows 10/11 client workstation.
  --require-whp       WHP capability, partition lifecycle AND a real guest vCPU
                      execution must all be present/verified. This is the
                      strict physical-host gate.
"""
import argparse
import json
import sys
from pathlib import Path


def main():
    ap = argparse.ArgumentParser(description="Verify nexus-engine physical-check evidence.")
    ap.add_argument("--evidence", default="native-win10-physical.json",
                    help="path to the evidence JSON written by `physical-check`")
    ap.add_argument("--require-win10", action="store_true",
                    help="fail unless the host is a Windows 10/11 client workstation")
    ap.add_argument("--require-whp", action="store_true",
                    help="strict physical-host gate: require WHP + real vCPU execution")
    args = ap.parse_args()

    path = Path(args.evidence)
    if not path.is_file():
        print(f"::error::Evidence file not found: {path}", flush=True)
        return 2
    try:
        ev = json.loads(path.read_text())
    except Exception as error:
        print(f"::error::Invalid evidence JSON: {error}", flush=True)
        return 2

    errors = []
    for key in ("schema", "host_os", "whp_capability_present",
                "whp_partition_lifecycle", "guest_vcpu_executed", "verdict", "boundary"):
        if key not in ev:
            errors.append(f"missing field: {key}")
    if errors:
        for error in errors:
            print(f"::error::{error}", flush=True)
        return 1

    if ev["schema"] != 1:
        errors.append(f"unexpected schema: {ev['schema']}")

    hos = ev["host_os"]
    if not isinstance(hos, dict):
        errors.append("host_os is not an object")
        hos = {}
    for key in ("major", "minor", "build", "platform", "product_type", "display", "is_windows_client"):
        if key not in hos:
            errors.append(f"host_os missing field: {key}")

    if args.require_win10 and hos.get("is_windows_client") is not True:
        errors.append(f"host is not a Windows 10/11 client workstation: {hos.get('display')}")

    if args.require_whp:
        if ev["whp_capability_present"] is not True:
            errors.append("require-whp: whp_capability_present is not true")
        if ev["whp_partition_lifecycle"] != "ok":
            errors.append(f"require-whp: whp_partition_lifecycle != ok: {ev['whp_partition_lifecycle']}")
        if ev["guest_vcpu_executed"] is not True:
            errors.append("require-whp: guest_vcpu_executed is not true")
        if ev["verdict"] != "pass":
            errors.append(f"require-whp: verdict != pass: {ev['verdict']}")

    if errors:
        for error in errors:
            print(f"::error::{error}", flush=True)
        return 1

    whp_state = ("present/verified" if (ev["whp_capability_present"] and ev["guest_vcpu_executed"])
                 else "absent (nested-virt CI VM or non-Windows)")
    print(f"::notice::PHYSICAL_CHECK_OK host={hos.get('display')} client={hos.get('is_windows_client')} whp={whp_state} verdict={ev['verdict']}", flush=True)
    print("Evidence schema validated: host OS identity + WHP probe + vCPU execution recorded; "
          "physical-host strict gate requires --require-win10 --require-whp", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
