// Two columns, three rows; coordinates match the DRC panel quad. The calls
// and their labels come from Rust (lib.rs c2_call_*), six a page.
// -1 outside, 0..5 picture slots, 6 previous page, 7 next page.
static int call_hit(int x, int y) {
    if(x<683 || x>=845 || y<34 || y>=446) return -1;
    int tx=(x-683)*256/162, ty=(y-34)*256/412;
    return ty>=216 ? 6+tx/128 : (ty/72)*2+tx/128;
}
static void call_labels(GX2Texture *tex, void *sim, int page) {
    int count=(int)c2_call_count(sim), pages=(count+5)/6;
    for(int slot=0;slot<6;++slot) {
        int i=c2_call_at(sim,(uint32_t)page,(uint32_t)slot); if(i<0) continue;
        char name[11];
        int selected=c2_call_label(sim,(uint32_t)i,name,sizeof(name))==1;
        int x=256+(slot%2)*128, y=(slot/2)*72;
        menu_box(tex,x,y,128,2,selected?240:70,selected?200:90,60);
        menu_box(tex,x,y+70,128,2,selected?240:70,selected?200:90,60);
        menu_box(tex,x,y,2,72,selected?240:70,selected?200:90,60);
        menu_box(tex,x+126,y,2,72,selected?240:70,selected?200:90,60);
        menu_text(tex,x+8,y+54,name,2,selected?240:255,selected?200:255,selected?70:255);
    }
    if(pages>1) {
        char at[16]; snprintf(at,sizeof(at),"%d/%d",page+1,pages);
        menu_text(tex,264,229,page?"PREV":at,2,240,200,70);
        menu_text(tex,392,229,page+1<pages?"NEXT":at,2,240,200,70);
    }
}
