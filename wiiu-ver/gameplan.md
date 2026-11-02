# Wii U development priorities

See the [port overview](README.md), [current frontend documentation](perf-probe/README.md),
[build instructions](../README.md#wii-u-from-the-command-line-demo), and
[technical assessment](../docs/WIIU.md). The earlier chronological roadmap is
preserved locally in `(testing)/historical-notes/gameplan.md`.

- Retain shared Carnivores 2, Ice Age, and MEE behavior and user-supplied data.
  Verify each supported game and representative mods independently.
- Support TV plus GamePad, TV-only, and GamePad-only play. Keep all essential
  menu and gameplay actions accessible through a controller; touch is optional.
- Validate complete hunts, weapons, animal behavior, audio, settings, saves,
  trophies, restart, and HOME/menu exit on physical hardware.
- Measure open terrain, dense scenery, movement, nearby animals, and both
  display modes before choosing graphics budgets or frame-rate targets.
  Separate Cemu observations from hardware results and CPU timings from GPU timings.
- Keep raw hardware captures and per-session notes local and ignored. Record
  reproducible conclusions and remaining defects in the maintained documentation.

The current port remains at `perf-probe/`; historical proof-of-concept probes
are local archives rather than additional supported build entry points.
