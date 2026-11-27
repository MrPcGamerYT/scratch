# Verified experimental downloads

Published release:
https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/releases/tag/nexus-verified-45648355363-45652854267

## Guest ZIP replacement

- Name: Nexus-Android11-Guest.zip
- Size: 2,069,741,280 bytes
- SHA256: b10f1810faa0a7cbca60d4f8bc2eda8df84ef2c02085fa919ab3ac792139beb8
- Contents: previously verified c032908 Android 11 guest, freshly repacked with ZIP DEFLATE.
- Guest functional build: 45648355363 (both no-NIC/no-ADB Home/shutdown gates passed).
- This archive does NOT contain the experimental APK/app-discovery guest changes.

## New Windows emulator

- Name: Nexus-Windows-Player.zip
- Size: 82,327,774 bytes
- SHA256: 245283dd335be4590082542124f7d833ed0b82bd708b0bed2e93d166c3d08906
- Successful build: 45652854267, commit 7e1d98a.
- Fresh engine build, Qt tests, native bridge/transfer/presentation-counter tests,
  bundled engine workload and portable ZIP checks passed.
- New native app/APK host functions require a corresponding validated guest;
  they are not activated merely by replacing the Windows executable.

## Delivery checks

Repack/delivery run 45663565890 succeeded:
- Original guest payload SHA256s checked before repacking.
- Python ZIP CRC check, 7-Zip test and unzip test passed.
- Published both ZIPs as direct GitHub release assets, not artifact wrappers.
- Downloaded the actual release ZIPs again; exact SHA256 and 7-Zip checks passed.

Use a current ZIP extractor such as 7-Zip. Wait for the complete download before
extracting. A truncated download is not repaired by repeatedly opening it.
Use only a new isolated instance; never replace an existing backing disk in place.

## New guest work remains blocked by real gates

The additional guest changes are deliberately NOT published. Run 45657328224
passed first boot and second-boot Home/package discovery, but failed native APK
installation. The installer was changed to use stdin streaming, with local tests
passing. Runs 45660839133 and 45663481567 then failed fresh Home provisioning
(resumed-Home timing and an am start -W wait respectively); they did not pass
packaging. Do not call the experimental guest a successful rebuild or claim
native APK installation has passed Android integration. No ANR gate was removed.

Windows acceleration, physical GPU/game performance, cursor calibration and
sound/network behavior remain unverified. These are experimental downloads.
