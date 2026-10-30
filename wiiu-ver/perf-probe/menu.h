/* The menus are drawn in Rust (src/menu.rs, src/ui.rs) at each display's
 * own resolution; C feeds them input and shows the result. */

/* Mirrors hunt_setup::HuntChoice. Flags are 0 or 1. */
typedef struct {
    uint32_t area, time, animals, weapons, radar, camo, scent, double_ammo, tranq, observer,
             run_pct, blood, death_anim, ambient, units_us;
} HuntChoice;
/* Button edges for c2_menu_input (menu.rs `button`). */
enum { MENU_UP = 1, MENU_DOWN = 2, MENU_LEFT = 4, MENU_RIGHT = 8, MENU_A = 16, MENU_B = 32,
       MENU_PLUS = 64, MENU_L = 128, MENU_R = 256 };
/* c2_menu_action (menu.rs `Action`). */
enum { MENU_NONE = 0, MENU_HUNT = 1, MENU_EXIT = 2, MENU_RESTART = 3, MENU_MAIN = 4 };
extern void *c2_menu_new(const uint8_t *, uint32_t);
extern int32_t c2_menu_file_name(uint32_t, char *, uint32_t);
extern int32_t c2_menu_set_file(void *, uint32_t, const uint8_t *, uint32_t);
extern void c2_menu_load_settings(void *, const uint8_t *, uint32_t);
extern uint32_t c2_menu_settings(void *, char *, uint32_t);
extern void c2_menu_open(void *, uint32_t, uint32_t, uint32_t);
extern uint32_t c2_menu_input(void *, uint32_t);
extern uint32_t c2_menu_touch(void *, float, float);
extern uint32_t c2_menu_action(void *);
extern void c2_menu_choice(void *, HuntChoice *);
extern void c2_menu_draw(void *, uint8_t *, uint32_t, uint32_t, uint32_t);
extern void c2_menu_drawn(void *);

static const char *menu_areas[] = {"DELPHAEUS HILLS", "FORT CISKIN", "VENGAR FJORDS", "MANYA JUNGLE", "MOUNT RAVAN"};

/* Result values also route the outer hunt-session loop. */
enum { HUNT_EXIT = 0, HUNT_RESTART = 1, HUNT_MAIN_MENU = 2 };

/* The in-hunt GamePad panels (call picker) still draw into 512x256 textures. */
static void menu_box(GX2Texture *t, int x, int y, int w, int h, uint8_t r, uint8_t g, uint8_t b) {
    uint8_t *p = t->surface.image;
    for (int yy = y; yy < y+h; ++yy) for (int xx = x; xx < x+w; ++xx) {
        if (xx < 0 || xx >= 512 || yy < 0 || yy >= 256) continue;
        uint8_t *q = p + (yy*t->surface.pitch + xx)*4; q[0]=r; q[1]=g; q[2]=b; q[3]=255;
    }
}
static void menu_text(GX2Texture *t, int x, int y, const char *text, int scale, uint8_t r, uint8_t g, uint8_t b) {
    for (int i=0; text[i]; ++i) {
        const char *ch = strchr(glyph_chars, text[i]); if (!ch) continue;
        int index = (int)(ch-glyph_chars);
        for (int yy=0; yy<7; ++yy) for (int xx=0; xx<5; ++xx)
            if (glyph_rows[index][yy] & (1 << (4-xx))) menu_box(t,x+(i*6+xx)*scale,y+yy*scale,scale,scale,r,g,b);
    }
}
