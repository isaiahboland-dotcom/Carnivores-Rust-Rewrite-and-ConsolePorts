# PC and Android controllers

These frontends use Xbox/Steam Deck face-button labels. Nintendo ports and
VR retain their existing mappings. Steam Deck rear buttons remain available
for a player's custom Steam Input profile.

| Control | Hunt action |
| --- | --- |
| Left stick / right stick | Move / look |
| A | Jump; double-tap within 0.3 seconds toggles flight in Advanced Observer |
| B | Alternate firing mode, for weapons supporting semi-auto and full-auto switching |
| X | Call; hold to choose a call. Advanced Observer: stop/resume time |
| Y | Next weapon; hold to choose a weapon |
| Left-stick click | Toggle sprint |
| Right-stick click | Cycle purchased lights: off, flashlight, night vision |
| LT / RT | Draw or holster / fire |
| LB / RB | Binoculars / map |
| D-pad Up / Down | Toggle crouch / next call |
| D-pad Left / Right | Reload / rack, for weapons supporting these actions |
| Start / Select | Pause / exit prompt |

Menus use A to confirm and B to cancel. In the weapon picker, B swaps
lethal/tranquilizer ammo when the hunt has split ammunition. In the call
picker, right-stick click changes the purchased flashlight's beam between
wide/short and narrow/far. These actions leave the picker open and consume
the input, so they do not trigger the normal gameplay action. Release the
picker button or press A to select; use the left stick or D-pad to choose.
When Button hints is enabled, the picker hints appear only for available
features, and the gameplay B hint only appears for supported firing modes.

Game settings:

- **Fire reloads when empty**: on by default on Android, off on PC. A fresh
  fire press with an empty MEE weapon requests its normal reload, waiting for
  the current weapon animation if necessary. Holding fire does not request a
  reload, and firing a loaded weapon remains immediate. Classic weapons
  retain their original automatic reload behavior.
- **Pause weapon / call pickers**: on by default. Turn off to let the hunt
  continue while choosing. Picker controls remain separate from gameplay.
