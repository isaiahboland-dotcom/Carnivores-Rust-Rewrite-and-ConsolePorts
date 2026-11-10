/* Behaviour runs in the original (docs/REGRESSION.md, "Behaviours"): the
 * hunter or one animal is logged every frame while the script watches it.
 *
 *   place X Y Z YAW   the hunter at X Y Z, looking along YAW (milliradians;
 *               only when the guessed PlayerAlpha global checks out)
 *   alarm AI [X Z]   the first living animal of AI (set down at X Z when
 *               given, as our engine's scenario animal): State 2 and a long
 *               AfraidTime, as a shot or a heard hunter (herbivores flee,
 *               predators charge); logged until `untrack`, or calmed as
 *               soon as it comes within NEAR_HUNTER units of the hunter, so a
 *               charge never kills him (our engine ends its watch there).
 *               Calmed, it goes back to X Z (our engine pins it there), or
 *               it would find the hunter again and kill him
 *   slay AI [X Z]   the first living animal of AI: Health 0 from a standstill
 *               (vspeed 0), the original's way into Animate*Dead
 *   track hunter, untrack
 *   pacify      from now on, every animal but the alarmed one is calmed
 *               each frame (State 0, no AfraidTime). The original places
 *               many roaming animals of each picked AI, and an alarmed one
 *               would find and kill the hunter; our engine has only the
 *               scenario's, pinned far away. In State 0 no AI kills.
 *
 * Log lines (parsed by src/original/behave.rs):
 *   frame F place X Z yaw set|kept
 *   frame F alarm|slay AI A index I        frame F alarm|slay AI A not found
 *   frame F track I PHASE FTIME X Z PX PZ SCALE   (I = -1: the hunter;
 *                                           SCALE x 1000, 1000 for him)
 *   frame F near                            the runner was calmed
 *   frame F hunter dead                     MyHealth reached 0 */
#include "hook.h"

#define NEAR_HUNTER 2048.0f

static int tracked = -2; /* -2: nothing, -1: the hunter */
static BOOL alarmed = FALSE, alive = FALSE, dead = FALSE, pacified = FALSE;
/* Where the alarmed animal goes back to when calmed. */
static BOOL home_set = FALSE;
static int home_x, home_z;

static int first_living(int ai) {
    int i, n = game_count();
    for (i = 0; i < n; i++)
        if (*ch_int(i, CH_AI) == ai && *ch_int(i, CH_HEALTH) > 0) return i;
    return -1;
}

static int pick(const char *what, int ai) {
    char m[96];
    int i;
    if (!game_ready()) {
        logs("behave: addresses missing or outside the executable\r\n");
        return -1;
    }
    i = first_living(ai);
    if (i < 0)
        wsprintfA(m, "frame %lu %s AI %d not found\r\n", frame, what, ai);
    else
        wsprintfA(m, "frame %lu %s AI %d index %d\r\n", frame, what, ai, i);
    logs(m);
    return i;
}

static void set_down(int i, BOOL at, int x, int z) {
    if (!at) return;
    *ch_float(i, CH_POSX) = (float)x;
    *ch_float(i, CH_POSZ) = (float)z;
    /* The game puts it back on the ground (ThinkY_Beta_Gamma). */
    *ch_float(i, CH_POSY) = player_y();
}

static void calm(int i) {
    *ch_int(i, CH_STATE) = 0;
    *ch_int(i, CH_AFRAID) = 0;
    *ch_float(i, CH_VSPEED) = 0.0f;
    set_down(i, home_set, home_x, home_z);
}

void behave_place(int x, int y, int z, int yaw_mrad) {
    char m[96];
    BOOL turned;
    if (!game_ready()) return;
    set_player((float)x, (float)y, (float)z);
    turned = set_player_alpha((float)yaw_mrad / 1000.0f);
    wsprintfA(m, "frame %lu place %d %d yaw %s\r\n", frame, x, z, turned ? "set" : "kept");
    logs(m);
}

void behave_alarm(int ai, BOOL at, int x, int z) {
    int i = pick("alarm", ai);
    if (i < 0) return;
    set_down(i, at, x, z);
    home_set = at;
    home_x = x;
    home_z = z;
    *ch_int(i, CH_STATE) = 2;
    *ch_int(i, CH_AFRAID) = 20000;
    *ch_int(i, CH_NOFIND) = 0;
    tracked = i;
    alarmed = TRUE;
}

void behave_slay(int ai, BOOL at, int x, int z) {
    int i = pick("slay", ai);
    if (i < 0) return;
    set_down(i, at, x, z);
    *ch_int(i, CH_HEALTH) = 0;
    *ch_float(i, CH_VSPEED) = 0.0f;
    tracked = i;
    alarmed = FALSE;
}

void behave_pacify(void) { pacified = TRUE; }

static void pacify_others(void) {
    int i, n = game_count();
    for (i = 0; i < n; i++) {
        if (alarmed && i == tracked) continue;
        if (*ch_int(i, CH_HEALTH) <= 0) continue;
        *ch_int(i, CH_STATE) = 0;
        *ch_int(i, CH_AFRAID) = 0;
    }
}

void behave_track_hunter(void) {
    tracked = -1;
    alarmed = FALSE;
}

void behave_untrack(void) {
    if (alarmed && tracked >= 0 && tracked < game_count()) calm(tracked);
    tracked = -2;
    alarmed = FALSE;
}

/* At the end of every frame, before the script's actions. */
void behave_step(void) {
    char m[160];
    int phase = 0, ftime = 0, scale = 1000;
    float x, z, px, pz, dx, dz;
    if (!game_ready()) return;
    /* MyHealth is 0 until the hunt starts. */
    if (player_health() > 0) alive = TRUE;
    if (alive && !dead && player_health() == 0) {
        dead = TRUE;
        wsprintfA(m, "frame %lu hunter dead\r\n", frame);
        logs(m);
    }
    if (pacified) pacify_others();
    if (tracked < -1) return;
    px = player_x();
    pz = player_z();
    if (tracked == -1) {
        x = px;
        z = pz;
    } else {
        if (tracked >= game_count()) return;
        phase = *ch_int(tracked, CH_PHASE);
        ftime = *ch_int(tracked, CH_FTIME);
        x = *ch_float(tracked, CH_POSX);
        z = *ch_float(tracked, CH_POSZ);
        scale = to_int(*ch_float(tracked, CH_SCALE) * 1000.0f);
    }
    wsprintfA(m, "frame %lu track %d %d %d %d %d %d %d %d\r\n", frame, tracked, phase, ftime,
              to_int(x), to_int(z), to_int(px), to_int(pz), scale);
    logs(m);
    dx = x - px;
    dz = z - pz;
    if (alarmed && dx * dx + dz * dz < NEAR_HUNTER * NEAR_HUNTER) {
        calm(tracked);
        alarmed = FALSE;
        tracked = -2;
        wsprintfA(m, "frame %lu near\r\n", frame);
        logs(m);
    }
}
