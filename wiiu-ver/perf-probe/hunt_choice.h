#ifndef C2_HUNT_CHOICE_H
#define C2_HUNT_CHOICE_H
#include <stdint.h>

/* Mirrors hunt_setup::HuntChoice. Flags are 0 or 1. */
typedef struct {
    uint32_t area, time;
    uint64_t animals;
    uint32_t weapons, radar, camo, scent, double_ammo, tranq, observer,
             run_pct, blood, death_anim, ambient, units_us, mode, survival, map_mobile, teleport, hints, all_huntable, dynamic_time, night_vision, flashlight,
             multi_ammo, half_tranq, free_observer;
} HuntChoice;

#endif
