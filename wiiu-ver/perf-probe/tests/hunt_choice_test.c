/* Match the repr(C) Rust HuntChoice, including optional night equipment. */
#include <stdint.h>
#include <stddef.h>
#include "../hunt_choice.h"
_Static_assert(sizeof(HuntChoice) == 104, "HuntChoice ABI size");
_Static_assert(offsetof(HuntChoice, night_vision) == 92, "night vision ABI offset");
_Static_assert(offsetof(HuntChoice, flashlight) == 96, "flashlight ABI offset");
int main(void) { return 0; }
