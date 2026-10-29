// Host check of the actual menu state, touch layout and raster bounds.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
typedef struct { struct { void *image; int pitch; unsigned imageSize; } surface; } GX2Texture;
#define GX2_INVALIDATE_MODE_CPU_TEXTURE 0
static void GX2Invalidate(int mode, void *p, unsigned n) { (void)mode; (void)p; (void)n; }
static const char glyph_chars[] = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ.:/-";
static const uint8_t glyph_rows[40][7] = {{31,31,31,31,31,31,31}};
#include "../menu.h"
#include "../call_picker.h"
static void choose(MenuState *m, int i) { m->focus=i; menu_input(m,0,0,1,0); }
int main(void) {
    for (int area=0;area<5;++area) for (int mask=1;mask<512;++mask) {
        MenuState m={0}; m.weapon=1; m.radar=1;
        choose(&m,0); assert(m.page==1);
        choose(&m,area); assert(m.area==area);
        choose(&m,6); assert(m.page==2);
        choose(&m,10); assert(m.page==2 && m.warning);
        for (int i=0;i<9;++i) if (mask & (1<<i)) choose(&m,i);
        assert(m.animal==mask);
        choose(&m,10); assert(m.page==3);
        choose(&m,6); assert(!m.radar);
        choose(&m,0); assert(!(m.weapon&1));
        choose(&m,8); assert(m.page==3 && m.warning);
        choose(&m,0); choose(&m,8); assert(m.page==4);
        menu_input(&m,0,0,0,1); assert(m.page==3 && m.animal==mask && m.area==area);
        choose(&m,8); choose(&m,1); assert(m.done);
    }
    GX2Texture tex={0}; tex.surface.pitch=512; tex.surface.imageSize=512*256*4;
    tex.surface.image=malloc(tex.surface.imageSize);
    for(int mask=1;mask<512;++mask) {
        int seen=0;
        for(int page=0;page<(call_count(mask)+5)/6;++page) {
            call_labels(&tex,mask,page,10);
            for(int slot=0;slot<6;++slot) {
                int x=683+((slot%2)*128+64)*162/256;
                int y=34+((slot/2)*72+36)*412/256;
                assert(call_hit(x,y)==slot);
                int i=call_index(mask,page,slot);
                if(i>=0) { assert(!(seen&(1<<i))); seen|=1<<i; assert(call_page_for(mask,i+10)==page); }
            }
        }
        assert(seen==mask);
    }
    assert(call_hit(700,420)==6 && call_hit(810,420)==7);
    assert(call_hit(682,100)==-1 && call_hit(845,100)==-1 && call_hit(700,446)==-1);
    uint8_t *art[17]; for (int i=0;i<17;++i) { art[i]=malloc(tex.surface.imageSize); memset(art[i],128,tex.surface.imageSize); }
    for (int page=0;page<6;++page) for (int focus=0;focus<menu_count(page);++focus) {
        MenuState m={0}; m.page=page; m.focus=focus; m.area=4; m.animal=511; m.weapon=1;
        MenuRect r=menu_rect(page,focus);
        assert(menu_hit(&m,r.x+r.w/2,r.y+r.h/2)==focus);
        menu_draw(&tex,art,&m);
        menu_input(&m,0,1,0,0); assert(m.focus==(focus+1)%menu_count(page));
        menu_input(&m,1,0,0,0); assert(m.focus==focus);
    }
    MenuState settings={0};
    choose(&settings,1); assert(settings.page==5);
    choose(&settings,0); assert(settings.blood_off && !settings.kill_anim_off);
    choose(&settings,1); assert(settings.blood_off && settings.kill_anim_off);
    menu_draw(&tex,art,&settings);
    choose(&settings,2); assert(settings.page==0 && settings.focus==1);
    choose(&settings,0); assert(settings.page==1 && settings.blood_off && settings.kill_anim_off);
    menu_input(&settings,0,0,0,1); choose(&settings,1);
    choose(&settings,0); choose(&settings,1);
    assert(!settings.blood_off && !settings.kill_anim_off);
    menu_input(&settings,0,0,0,1); assert(settings.page==0 && settings.focus==1);
    assert(death_hit(0,0)==-1);
    for (int focus=0;focus<2;++focus) {
        DeathMenu d={.focus=focus,.shots=123,.kills=4};
        MenuRect r=death_rect(focus);
        assert(death_hit(r.x+r.w/2,r.y+r.h/2)==focus);
        assert(death_hit(r.x+r.w,r.y)==-1);
        death_draw(&tex,&d);
        death_input(&d,0,1,0); assert(d.focus==1-focus && !d.action);
        death_input(&d,1,0,0); assert(d.focus==focus);
        death_input(&d,0,0,1); assert(d.action==(focus?HUNT_MAIN_MENU:HUNT_RESTART));
    }
    for(int i=0;i<17;++i) free(art[i]);
    free(tex.surface.image);
    return 0;
}
