# 2026-11-27 — Clean-room denylist filled in

- `docs/cleanroom-denylist.txt` now holds the terms supplied by the spec room with spec
  gate-1.4: whole words, case-insensitive; a hit on the first term alone, which is also an
  ordinary English word, is checked by hand. The file is the only place these terms may appear.
- The clean-room auditor matches whole words and skips the denylist file itself.
