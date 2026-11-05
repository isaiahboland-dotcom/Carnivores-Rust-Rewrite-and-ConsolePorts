/* Match the repr(C) Rust HuntChoice, including optional night equipment. */
#include <stdint.h>
#include <stddef.h>
#include "../hunt_choice.h"
_Static_assert(sizeof(HuntChoice) == 112, "HuntChoice ABI size");
_Static_assert(offsetof(HuntChoice, night_vision) == 92, "night vision ABI offset");
_Static_assert(offsetof(HuntChoice, flashlight) == 96, "flashlight ABI offset");
_Static_assert(offsetof(HuntChoice, multi_ammo) == 100, "multi ammo ABI offset");
_Static_assert(offsetof(HuntChoice, free_observer) == 108, "advanced observer ABI offset");
int main(void) { return 0; }
