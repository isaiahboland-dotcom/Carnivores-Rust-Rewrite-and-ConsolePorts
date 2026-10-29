// Wii U rendition of the 3DS hunt setup, limited to assets supported by the probe.
typedef struct { int page, focus, area, animal, weapon, radar, done, warning, blood_off, kill_anim_off; } MenuState;
typedef struct { int x, y, w, h; } MenuRect;
static int menu_count(int page) { static const int counts[] = {2,7,11,9,2,3}; return counts[page]; }
static MenuRect menu_rect(int page, int item) {
    if (page == 0) return (MenuRect){25,106+item*48,160,38};
    if (page == 5) return item == 2 ? (MenuRect){12,217,110,30} : (MenuRect){64,65+item*60,384,44};
    if (page == 4) return item == 0 ? (MenuRect){12,217,110,30} : (MenuRect){386,217,114,30};
    if (page == 3 && item < 6) return (MenuRect){268+(item%2)*78,48+(item/2)*51,52,48};
    if (page == 3 && item == 6) return (MenuRect){432,65,72,100};
    if (page == 1 && item < 5) return (MenuRect){235 + (item % 3)*96, 49 + (item / 3)*74, 70, 70};
    if (page == 2 && item < 9) return (MenuRect){248 + (item % 3)*84, 42 + (item / 3)*52, 48, 48};
    int back_item = menu_count(page) - 2;
    return item == back_item ? (MenuRect){12,217,110,30} : (MenuRect){386,217,114,30};
}
static int menu_hit(const MenuState *m, int x, int y) {
    for (int i = 0; i < menu_count(m->page); ++i) {
        MenuRect r = menu_rect(m->page, i);
        if (x >= r.x && x < r.x+r.w && y >= r.y && y < r.y+r.h) return i;
    }
    return -1;
}
static void menu_input(MenuState *m, int prev, int next, int activate, int back) {
    int count = menu_count(m->page);
    if (prev) m->focus = (m->focus + count - 1) % count;
    if (next) m->focus = (m->focus + 1) % count;
    if (m->page == 5) {
        if (back || (activate && m->focus == 2)) { m->page = 0; m->focus = 1; }
        else if (activate && m->focus == 0) m->blood_off = !m->blood_off;
        else if (activate && m->focus == 1) m->kill_anim_off = !m->kill_anim_off;
        return;
    }
    if (back && m->page) { --m->page; m->focus = 0; m->warning = 0; return; }
    if (!activate) return;
    if (m->page == 0) { m->page = m->focus == 1 ? 5 : 1; m->focus = 0; return; }
    int back_item = count - 2;
    int next_item = menu_count(m->page) - 1;
    if (m->focus == back_item) { --m->page; m->focus = 0; m->warning = 0; }
    else if (m->focus == next_item) {
        if ((m->page >= 2 && !m->animal) || (m->page >= 3 && !m->weapon)) { m->warning = 1; return; }
        m->warning = 0;
        if (m->page == 4) m->done = 1; else { ++m->page; m->focus = 0; }
    } else if (m->page == 1 && m->focus < 5) m->area = m->focus;
    else if (m->page == 2 && m->focus < 9) { m->animal ^= 1 << m->focus; m->warning = 0; }
    else if (m->page == 3 && m->focus < 6) { m->weapon ^= 1 << m->focus; m->warning = 0; }
    else if (m->page == 3 && m->focus == 6) m->radar = !m->radar;
    else if (m->page == 3 && m->focus == 6) m->radar = !m->radar;
}
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
static void menu_button(GX2Texture *t, MenuRect q, const char *label, int focus) {
    menu_box(t,q.x,q.y,q.w,q.h,focus?240:90,focus?200:110,focus?70:68);
    menu_box(t,q.x+2,q.y+2,q.w-4,q.h-4,32,44,30);
    int x=q.x+(q.w-(int)strlen(label)*12)/2;
    menu_text(t,x,q.y+(q.h-14)/2,label,2,255,255,255);
}
static int menu_in_hex(int x, int y, int cx, int cy, int r) {
    int dx=abs(x-cx), dy=abs(y-cy);
    return dy*1000 <= r*866 && dx*1732+dy*1000 <= r*1732;
}
static void menu_hex(GX2Texture *t, const uint8_t *art, int cx, int cy, int r, int on, int focus) {
    uint8_t *dst = t->surface.image;
    for (int y=cy-r; y<=cy+r; ++y) for (int x=cx-r; x<=cx+r; ++x) {
        if (x<0 || x>=512 || y<0 || y>=256 || !menu_in_hex(x,y,cx,cy,r)) continue;
        uint8_t *p=dst+(y*t->surface.pitch+x)*4;
        if (!menu_in_hex(x,y,cx,cy,r-4)) {
            int cursor = focus && !menu_in_hex(x,y,cx,cy,r-2);
            p[0]=cursor?255:on?240:70; p[1]=cursor?255:on?200:90; p[2]=cursor?255:on?70:60; p[3]=255; continue;
        }
        int sx=(x-(cx-r))*511/(2*r), sy=(y-(cy-r))*255/(2*r);
        const uint8_t *src=art+(sy*512+sx)*4; memcpy(p,src,4);
        if (!on) { p[0]/=2; p[1]/=2; p[2]/=2; }
    }
}
static const char *menu_areas[] = {"DELPHAEUS HILLS", "FORT CISKIN", "VENGAR FJORDS", "MANYA JUNGLE", "MOUNT RAVAN"};
static const char *menu_animals[] = {"PARASAUROLOPHUS", "ANKYLOSAURUS", "STEGOSAURUS", "ALLOSAURUS", "CHASMOSAURUS", "VELOCIRAPTOR", "SPINOSAURUS", "CERATOSAURUS", "T-REX"};
static void menu_draw(GX2Texture *tex, uint8_t **art, const MenuState *m) {
    if (m->page == 0) {
        for (int y=0;y<256;++y) memcpy((uint8_t*)tex->surface.image+y*tex->surface.pitch*4,art[0]+y*512*4,512*4);
        menu_box(tex,10,91,190,151,10,14,10);
        menu_button(tex,menu_rect(0,0),"PLAY",m->focus==0);
        menu_button(tex,menu_rect(0,1),"SETTINGS",m->focus==1);
    } else if (m->page == 5) {
        menu_box(tex,0,0,512,256,16,22,16);
        menu_box(tex,0,0,512,32,32,44,30);
        menu_text(tex,12,9,"SETTINGS",2,240,200,70);
        menu_button(tex,menu_rect(5,0),m->blood_off?"BLOOD: OFF":"BLOOD: ON",m->focus==0);
        menu_button(tex,menu_rect(5,1),m->kill_anim_off?"KILL ANIMATIONS: OFF":"KILL ANIMATIONS: ON",m->focus==1);
        menu_text(tex,64,184,"APPLIES TO NEW HUNTS AND RESTARTS",1,160,210,130);
        menu_button(tex,menu_rect(5,2),"BACK",m->focus==2);
    } else {
        menu_box(tex,0,0,512,256,16,22,16);
        menu_box(tex,0,0,512,32,32,44,30);
        const char *heading[] = {"", "SELECT AREA", "SELECT DINOSAURS", "WEAPONS AND GEAR", "HUNT OPTIONS"};
        menu_text(tex,12,9,heading[m->page],2,240,200,70);
        menu_text(tex,12,42,"HUNT SUMMARY",2,160,210,130);
        menu_box(tex,12,64,210,134,32,44,30);
        menu_text(tex,22,72,menu_areas[m->area],1,255,255,255);
        menu_text(tex,22,86,"DAY",1,160,210,130);
        for (int i=0,row=0;i<9;++i) if (m->animal & (1 << i)) menu_text(tex,22,98+row++*9,menu_animals[i],1,255,255,255);
        menu_text(tex,22,183,m->weapon?"WEAPONS SELECTED":"NO WEAPON",1,255,255,255);
        menu_text(tex,104,183,m->radar?"RADAR ON":"RADAR OFF",1,255,255,255);
        menu_text(tex,22,201,"POINTS 9999",1,240,200,70);
        if (m->page == 1 || m->page == 2) {
            int count=m->page==1?5:9;
            for (int i=0;i<count;++i) {
                MenuRect r=menu_rect(m->page,i);
                menu_hex(tex,art[(m->page==1?1:6)+i],r.x+r.w/2,r.y+r.h/2,r.w/2,m->page==1?m->area==i:!!(m->animal&(1<<i)),m->focus==i);
            }
            if (m->focus<count) menu_text(tex,245,201,m->page==1?menu_areas[m->focus]:menu_animals[m->focus],1,255,255,255);
        } else if (m->page == 3) {
            for(int i=0;i<6;++i) { MenuRect r=menu_rect(3,i); menu_hex(tex,art[15+i],r.x+r.w/2,r.y+r.h/2,25,!!(m->weapon&(1<<i)),m->focus==i); }
            menu_text(tex,452,105,m->radar?"RADAR ON":"RADAR OFF",1,255,255,255);
            menu_text(tex,250,202,"SELECT WEAPONS TO CARRY",1,160,210,130);
        } else { menu_text(tex,275,86,"READY TO HUNT",2,160,210,130); menu_text(tex,270,116,menu_areas[m->area],1,255,255,255); }
        int back=menu_count(m->page)-2, next=menu_count(m->page)-1;
        menu_button(tex,menu_rect(m->page,back),"BACK",m->focus==back);
        menu_button(tex,menu_rect(m->page,next),m->page==4?"HUNT":"NEXT",m->focus==next);
        if (m->warning) { menu_box(tex,240,196,272,17,16,22,16); menu_text(tex,245,201,"CHOOSE TARGET AND WEAPON",1,240,80,60); }
    }
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE,tex->surface.image,tex->surface.imageSize);
}

// Result values also route the outer hunt-session loop.
enum { HUNT_EXIT = 0, HUNT_RESTART = 1, HUNT_MAIN_MENU = 2 };
typedef struct { int focus, action; uint32_t shots, kills; } DeathMenu;
static MenuRect death_rect(int item) { return (MenuRect){124, item ? 181 : 139, 264, 34}; }
static int death_hit(int x, int y) {
    for (int i=0;i<2;++i) {
        MenuRect r=death_rect(i);
        if (x>=r.x && x<r.x+r.w && y>=r.y && y<r.y+r.h) return i;
    }
    return -1;
}
static void death_input(DeathMenu *m, int prev, int next, int activate) {
    if (prev || next) m->focus ^= 1;
    if (activate) m->action = m->focus ? HUNT_MAIN_MENU : HUNT_RESTART;
}
static void death_draw(GX2Texture *tex, const DeathMenu *m) {
    menu_box(tex,0,0,512,256,16,22,16);
    menu_box(tex,34,30,444,196,32,44,30);
    menu_text(tex,94,46,"YOU WERE KILLED",4,240,80,60);
    menu_text(tex,130,89,"YOUR HUNT HAS ENDED",2,240,200,70);
    char line[48]; snprintf(line,sizeof(line),"SHOTS %u   KILLS %u",m->shots,m->kills);
    menu_text(tex,160,115,line,1,255,255,255);
    menu_button(tex,death_rect(0),"RESTART HUNT",m->focus==0);
    menu_button(tex,death_rect(1),"MAIN MENU",m->focus==1);
    menu_text(tex,130,238,"D-PAD / A OR TAP TO SELECT",1,160,210,130);
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE,tex->surface.image,tex->surface.imageSize);
}
