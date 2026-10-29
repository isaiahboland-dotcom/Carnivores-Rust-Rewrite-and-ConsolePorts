// Two columns, three rows; coordinates match the DRC panel quad.
static int call_count(int mask) { int n=0; for(int i=0;i<9;++i) n+=(mask>>i)&1; return n; }
static int call_index(int mask, int page, int slot) {
    if(page<0 || slot<0 || slot>=6) return -1;
    int rank=page*6+slot;
    for(int i=0;i<9;++i) if(mask & (1<<i)) { if(rank--==0) return i; }
    return -1;
}
static int call_page_for(int mask, int ai) {
    int rank=0; for(int i=0;i<9;++i) if(mask & (1<<i)) { if(i+10==ai) return rank/6; ++rank; }
    return 0;
}
// -1 outside, 0..5 picture slots, 6 previous page, 7 next page.
static int call_hit(int x, int y) {
    if(x<683 || x>=845 || y<34 || y>=446) return -1;
    int tx=(x-683)*256/162, ty=(y-34)*256/412;
    return ty>=216 ? 6+tx/128 : (ty/72)*2+tx/128;
}
static void call_labels(GX2Texture *tex, int mask, int page, int ai) {
    static const char *names[]={"PARA","ANKYLO","STEGO","ALLO","CHASMO","VELO","SPINO","CERATO","T-REX"};
    for(int slot=0;slot<6;++slot) {
        int i=call_index(mask,page,slot); if(i<0) continue;
        int x=256+(slot%2)*128, y=(slot/2)*72;
        int selected=i+10==ai;
        menu_box(tex,x,y,128,2,selected?240:70,selected?200:90,60);
        menu_box(tex,x,y+70,128,2,selected?240:70,selected?200:90,60);
        menu_box(tex,x,y,2,72,selected?240:70,selected?200:90,60);
        menu_box(tex,x+126,y,2,72,selected?240:70,selected?200:90,60);
        menu_text(tex,x+8,y+54,names[i],2,selected?240:255,selected?200:255,selected?70:255);
    }
    if(call_count(mask)>6) {
        menu_text(tex,264,229,page?"PREV":"1/2",2,240,200,70);
        menu_text(tex,392,229,page?"2/2":"NEXT",2,240,200,70);
    }
}
