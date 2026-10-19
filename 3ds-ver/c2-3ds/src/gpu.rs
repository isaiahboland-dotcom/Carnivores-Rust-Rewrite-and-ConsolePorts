//! A thin layer over citro3d's C API for what the game draws: two shaders
//! (textured world geometry, flat-coloured geometry), textures from the
//! cooked pack, vertex buffers in GPU-visible memory, and
//! the few render states the original uses.

use std::ffi::{c_void, CString};
use std::mem::{size_of, zeroed};

use c2_formats::cooked::CookedTexture;
use citro3d_sys as c3d;
use ctru_sys as sys;
use glam::Mat4;

static SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shaders.shbin"));

/// 24 bytes: position, texture coordinate, 4 light bytes (see the shader).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub lit: [u8; 4],
}

/// 16 bytes, for the flat shader: position and RGBA colour.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct FlatVertex {
    pub pos: [f32; 3],
    pub rgba: [u8; 4],
}

/// Memory the GPU can read (the linear heap).
pub struct LinearBuf<T: Copy> {
    ptr: *mut T,
    len: usize,
    cap: usize,
}

impl<T: Copy> LinearBuf<T> {
    pub fn new(cap: usize) -> Option<Self> {
        let ptr = unsafe { sys::linearAlloc(cap.max(1) * size_of::<T>()) } as *mut T;
        (!ptr.is_null()).then_some(Self { ptr, len: 0, cap })
    }

    pub fn from_slice(data: &[T]) -> Option<Self> {
        let mut b = Self::new(data.len())?;
        b.write(data);
        Some(b)
    }

    /// Replaces the contents (up to the capacity) and flushes them to memory.
    pub fn write(&mut self, data: &[T]) {
        self.len = 0;
        self.append(data);
    }

    /// Adds `data` after the current contents; returns where it starts, or
    /// `None` if it doesn't fit. A buffer drawn several times in a frame is
    /// filled this way: the GPU reads it only when the frame is submitted.
    pub fn append(&mut self, data: &[T]) -> Option<usize> {
        let at = self.len;
        if at + data.len() > self.cap {
            return None;
        }
        unsafe {
            let dst = self.ptr.add(at);
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
            let _ = sys::GSPGPU_FlushDataCache(
                dst as *const c_void,
                (data.len() * size_of::<T>()) as u32,
            );
        }
        self.len = at + data.len();
        Some(at)
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn cap(&self) -> usize {
        self.cap
    }
}

impl<T: Copy> Drop for LinearBuf<T> {
    fn drop(&mut self) {
        unsafe { sys::linearFree(self.ptr as *mut c_void) };
    }
}

pub struct Texture {
    // citro3d keeps a pointer to the bound texture, so it must not move.
    tex: Box<c3d::C3D_Tex>,
}

impl Texture {
    pub fn new(t: &CookedTexture) -> Option<Self> {
        let mut tex: Box<c3d::C3D_Tex> = Box::new(unsafe { zeroed() });
        unsafe {
            if !c3d::C3D_TexInit(&mut *tex, t.width, t.height, t.format as sys::GPU_TEXCOLOR) {
                return None;
            }
            c3d::C3D_TexUpload(&mut *tex, t.data.as_ptr() as *const c_void);
            c3d::C3D_TexSetFilter(&mut *tex, sys::GPU_LINEAR, sys::GPU_LINEAR);
            c3d::C3D_TexSetWrap(&mut *tex, sys::GPU_CLAMP_TO_EDGE, sys::GPU_CLAMP_TO_EDGE);
        }
        Some(Self { tex })
    }

    /// Tile the texture instead of clamping (the scrolling sky).
    pub fn repeat(&mut self) {
        unsafe { c3d::C3D_TexSetWrap(&mut *self.tex, sys::GPU_REPEAT, sys::GPU_REPEAT) };
    }

    pub fn bind(&self) {
        unsafe { c3d::C3D_TexBind(0, &*self.tex as *const _ as *mut _) };
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { c3d::C3D_TexDelete(&mut *self.tex) };
    }
}

/// How the light bytes of the vertices are read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lit {
    /// 0..255 (terrain light).
    Unsigned,
    /// -128..127 (`CalcLights` per quarter turn).
    Signed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Opaque, cut-out texels discarded.
    Cutout,
    /// Mixed with what is behind by the alpha (water, transparent faces).
    Alpha,
    /// Added to what is behind (sun, water rings).
    Add,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prog {
    World,
    Flat,
}

/// Converts a glam (column-major) matrix to citro3d's layout: rows, each
/// stored `w, z, y, x`.
pub fn c3d_mtx(m: &Mat4) -> c3d::C3D_Mtx {
    let mut out: c3d::C3D_Mtx = unsafe { zeroed() };
    let cols = m.to_cols_array_2d();
    for row in 0..4 {
        for col in 0..4 {
            unsafe { out.m[row * 4 + (3 - col)] = cols[col][row] };
        }
    }
    out
}

pub struct Gpu {
    target: *mut c3d::C3D_RenderTarget,
    bottom: *mut c3d::C3D_RenderTarget,
    world: Box<sys::shaderProgram_s>,
    flat: Box<sys::shaderProgram_s>,
    _dvlb: *mut sys::DVLB_s,
    _code: Vec<u32>,
    u_proj: i32,
    u_mv: i32,
    u_light: i32,
    u_lsel: i32,
    u_fog: i32,
    u_flat_proj: i32,
    u_flat_mv: i32,
    fog_rgb: u32,
    /// Current program and vertex layout.
    prog: Prog,
    lit: Option<Lit>,
    // Uniform values, re-sent when the program changes (the two programs
    // may place uniforms and constants in the same registers).
    proj: c3d::C3D_Mtx,
    mv: c3d::C3D_Mtx,
    light: [f32; 4],
    lsel: [f32; 4],
    fog: [f32; 4],
    /// The world texenv's alpha (see [`Gpu::texenv`]).
    tex_alpha: Option<u8>,
    night: bool,
}

pub const TOP_W: usize = 400;
pub const TOP_H: usize = 240;
pub const BOTTOM_W: usize = 320;
pub const BOTTOM_H: usize = 240;

/// `out`: the screen's framebuffer format (the top is RGB8; the bottom is
/// RGB565, as libctru's console sets it up).
const fn transfer_flags(out: u8) -> u32 {
    c3d::GX_TRANSFER_FLIP_VERT(false)
        | c3d::GX_TRANSFER_OUT_TILED(false)
        | c3d::GX_TRANSFER_RAW_COPY(false)
        | c3d::GX_TRANSFER_IN_FORMAT(sys::GX_TRANSFER_FMT_RGBA8)
        | c3d::GX_TRANSFER_OUT_FORMAT(out)
        | c3d::GX_TRANSFER_SCALING(sys::GX_TRANSFER_SCALE_NO)
}

impl Gpu {
    pub fn new() -> Option<Self> {
        unsafe {
            // Room for a few thousand draw calls per frame.
            if !c3d::C3D_Init(0x10_0000) {
                return None;
            }
            let target = c3d::C3D_RenderTargetCreate(
                TOP_H as i32,
                TOP_W as i32,
                sys::GPU_RB_RGBA8,
                c3d::C3D_DEPTHTYPE {
                    __e: sys::GPU_RB_DEPTH24_STENCIL8,
                },
            );
            if target.is_null() {
                return None;
            }
            c3d::C3D_RenderTargetSetOutput(
                target,
                sys::GFX_TOP,
                sys::GFX_LEFT,
                transfer_flags(sys::GX_TRANSFER_FMT_RGB8),
            );
            let bottom = c3d::C3D_RenderTargetCreate(
                BOTTOM_H as i32,
                BOTTOM_W as i32,
                sys::GPU_RB_RGBA8,
                c3d::C3D_DEPTHTYPE { __i: -1 },
            );
            if bottom.is_null() {
                return None;
            }
            c3d::C3D_RenderTargetSetOutput(
                bottom,
                sys::GFX_BOTTOM,
                sys::GFX_LEFT,
                transfer_flags(sys::GX_TRANSFER_FMT_RGB565),
            );
            // The console drew it single-buffered; the GPU draws a frame at
            // a time.
            sys::gfxSetDoubleBuffering(sys::GFX_BOTTOM, true);

            // DVLB_ParseFile keeps pointers into the (word-aligned) code.
            let mut code = vec![0u32; SHADER.len().div_ceil(4)];
            std::ptr::copy_nonoverlapping(
                SHADER.as_ptr(),
                code.as_mut_ptr() as *mut u8,
                SHADER.len(),
            );
            let dvlb = sys::DVLB_ParseFile(code.as_mut_ptr(), SHADER.len() as u32);
            if dvlb.is_null() {
                return None;
            }
            if (*dvlb).numDVLE < 2 {
                return None;
            }
            let program = |i: usize| {
                let mut p: Box<sys::shaderProgram_s> = Box::new(zeroed());
                let _ = sys::shaderProgramInit(&mut *p);
                let _ = sys::shaderProgramSetVsh(&mut *p, (*dvlb).DVLE.add(i));
                p
            };
            let (world, flat) = (program(0), program(1));
            let loc = |p: &sys::shaderProgram_s, name: &str| {
                let c = CString::new(name).unwrap_or_default();
                i32::from(sys::shaderInstanceGetUniformLocation(
                    p.vertexShader,
                    c.as_ptr(),
                ))
            };
            let mut gpu = Self {
                target,
                bottom,
                u_proj: loc(&world, "projection"),
                u_mv: loc(&world, "modelView"),
                u_light: loc(&world, "light"),
                u_lsel: loc(&world, "lsel"),
                u_fog: loc(&world, "fog"),
                u_flat_proj: loc(&flat, "projection"),
                u_flat_mv: loc(&flat, "modelView"),
                world,
                flat,
                _dvlb: dvlb,
                _code: code,
                fog_rgb: 0,
                prog: Prog::World,
                lit: None,
                proj: zeroed(),
                mv: zeroed(),
                light: [0.0; 4],
                lsel: [0.0; 4],
                fog: [0.0; 4],
                tex_alpha: None,
                night: false,
            };
            gpu.bind(Prog::World);
            Some(gpu)
        }
    }

    /// Starts a frame on the top screen, cleared to `rgb`.
    pub fn begin(&mut self, rgb: [u8; 3]) {
        let [r, g, b] = rgb.map(u32::from);
        unsafe {
            c3d::C3D_FrameBegin(c3d::C3D_FRAME_SYNCDRAW as u8);
            c3d::C3D_RenderTargetClear(
                self.target,
                c3d::C3D_CLEAR_ALL,
                r << 24 | g << 16 | b << 8 | 0xff,
                0,
            );
            c3d::C3D_FrameDrawOn(self.target);
            c3d::C3D_CullFace(sys::GPU_CULL_NONE);
        }
        self.bind(Prog::World);
        self.depth(true, false);
        self.blend(Blend::Cutout);
        self.texenv(None);
    }

    /// 2D drawing in screen pixels (x right, y down) on the top screen, over
    /// the world, or on the bottom screen (cleared to `rgb`).
    pub fn begin_2d(&mut self, bottom: Option<[u8; 3]>) {
        let w = if let Some(rgb) = bottom {
            let [r, g, b] = rgb.map(u32::from);
            unsafe {
                c3d::C3D_RenderTargetClear(
                    self.bottom,
                    c3d::C3D_CLEAR_COLOR,
                    r << 24 | g << 16 | b << 8 | 0xff,
                    0,
                );
                c3d::C3D_FrameDrawOn(self.bottom);
            }
            BOTTOM_W
        } else {
            TOP_W
        };
        let mut p: c3d::C3D_Mtx = unsafe { zeroed() };
        unsafe { c3d::Mtx_OrthoTilt(&mut p, 0.0, w as f32, TOP_H as f32, 0.0, 0.0, 1.0, true) };
        self.proj = p;
        self.unif_mtx(self.u_proj, self.u_flat_proj, &p);
        self.set_model_view(&Mat4::IDENTITY);
        self.set_light(1.0, None);
        self.no_fog();
        self.depth_off();
        self.blend(Blend::Alpha);
        self.night = false;
    }

    /// Textured 2D: colour and alpha = texture x `rgba`, no fog. Undone by
    /// the next [`texenv`](Self::texenv).
    pub fn texenv_tint(&mut self, rgba: [u8; 4]) {
        if self.prog != Prog::World {
            self.bind(Prog::World);
        }
        self.tex_alpha = None;
        unsafe {
            let env = c3d::C3D_GetTexEnv(0);
            c3d::C3D_TexEnvInit(env);
            c3d::C3D_TexEnvSrc(env, c3d::C3D_Both, sys::GPU_TEXTURE0, sys::GPU_CONSTANT, 0);
            c3d::C3D_TexEnvFunc(env, c3d::C3D_Both, sys::GPU_MODULATE);
            let [r, g, b, a] = rgba.map(u32::from);
            c3d::C3D_TexEnvColor(env, a << 24 | b << 16 | g << 8 | r);
            c3d::C3D_TexEnvInit(c3d::C3D_GetTexEnv(1));
            c3d::C3D_TexEnvInit(c3d::C3D_GetTexEnv(2));
        }
    }

    pub fn end(&mut self) {
        unsafe { c3d::C3D_FrameEnd(0) };
    }

    /// Switches program and sends it the current uniforms.
    fn bind(&mut self, p: Prog) {
        unsafe {
            let v = sys::GPU_VERTEX_SHADER;
            match p {
                Prog::World => {
                    c3d::C3D_BindProgram(&mut *self.world);
                    c3d::C3D_FVUnifMtx4x4(v, self.u_proj, &self.proj);
                    c3d::C3D_FVUnifMtx4x4(v, self.u_mv, &self.mv);
                    for (u, x) in [
                        (self.u_light, self.light),
                        (self.u_lsel, self.lsel),
                        (self.u_fog, self.fog),
                    ] {
                        c3d::C3D_FVUnifSet(v, u, x[0], x[1], x[2], x[3]);
                    }
                }
                Prog::Flat => {
                    c3d::C3D_BindProgram(&mut *self.flat);
                    c3d::C3D_FVUnifMtx4x4(v, self.u_flat_proj, &self.proj);
                    c3d::C3D_FVUnifMtx4x4(v, self.u_flat_mv, &self.mv);
                }
            }
        }
        self.prog = p;
        self.lit = None;
    }

    fn unif_mtx(&mut self, world: i32, flat: i32, m: &c3d::C3D_Mtx) {
        let u = if self.prog == Prog::World {
            world
        } else {
            flat
        };
        unsafe { c3d::C3D_FVUnifMtx4x4(sys::GPU_VERTEX_SHADER, u, m) };
    }

    fn unif_vec(&mut self, u: i32, x: [f32; 4]) {
        if self.prog == Prog::World {
            unsafe { c3d::C3D_FVUnifSet(sys::GPU_VERTEX_SHADER, u, x[0], x[1], x[2], x[3]) };
        }
    }

    /// Perspective for the top screen: vertical field of view `fovy`.
    pub fn set_projection(&mut self, fovy: f32, near: f32, far: f32) {
        let mut p: c3d::C3D_Mtx = unsafe { zeroed() };
        unsafe {
            c3d::Mtx_PerspTilt(
                &mut p,
                fovy,
                c3d::C3D_AspectRatioTop as f32,
                near,
                far,
                false,
            );
        }
        self.proj = p;
        self.unif_mtx(self.u_proj, self.u_flat_proj, &p);
    }

    pub fn set_model_view(&mut self, mv: &Mat4) {
        let m = c3d_mtx(mv);
        self.mv = m;
        self.unif_mtx(self.u_mv, self.u_flat_mv, &m);
    }

    /// Base light (0..1) and which light byte to add (0..=3), if any.
    pub fn set_light(&mut self, base: f32, select: Option<usize>) {
        let mut s = [0.0f32; 4];
        if let Some(i) = select {
            s[i & 3] = 1.0 / 255.0;
        }
        self.light = [base, 0.0, 0.0, 0.0];
        self.lsel = s;
        self.unif_vec(self.u_light, self.light);
        self.unif_vec(self.u_lsel, s);
    }

    /// Linear fog from `start` to `end` (world units from the eye) to
    /// `rgb`. The GPU's own fog table works on depth-buffer values, which
    /// are too non-linear at this view distance, so the shader computes it.
    pub fn set_fog(&mut self, rgb: [u8; 3], start: f32, end: f32) {
        let k = 1.0 / (end - start).max(1.0);
        self.fog = [k, -start * k, 0.0, 0.0];
        self.unif_vec(self.u_fog, self.fog);
        let [r, g, b] = rgb.map(u32::from);
        self.fog_rgb = 0xff00_0000 | b << 16 | g << 8 | r;
    }

    /// No fog (the view model, the sun).
    pub fn no_fog(&mut self) {
        self.fog = [0.0; 4];
        self.unif_vec(self.u_fog, self.fog);
    }

    /// No depth test or writes (the sky, drawn first).
    pub fn depth_off(&mut self) {
        unsafe { c3d::C3D_DepthTest(false, sys::GPU_ALWAYS, sys::GPU_WRITE_COLOR) };
    }

    /// Depth test on; `write` off for blended things; `near` squeezes depth
    /// into the front 2% so the view model is never hidden by the world.
    pub fn depth(&mut self, write: bool, near: bool) {
        unsafe {
            c3d::C3D_DepthTest(
                true,
                sys::GPU_GREATER,
                if write {
                    sys::GPU_WRITE_ALL
                } else {
                    sys::GPU_WRITE_COLOR
                },
            );
            if near {
                c3d::C3D_DepthMap(true, -0.02, 0.98);
            } else {
                c3d::C3D_DepthMap(true, -1.0, 0.0);
            }
        }
    }

    pub fn blend(&mut self, b: Blend) {
        unsafe {
            match b {
                Blend::Cutout => {
                    c3d::C3D_AlphaTest(true, sys::GPU_GREATER, 127);
                    c3d::C3D_AlphaBlend(
                        sys::GPU_BLEND_ADD,
                        sys::GPU_BLEND_ADD,
                        sys::GPU_ONE,
                        sys::GPU_ZERO,
                        sys::GPU_ONE,
                        sys::GPU_ZERO,
                    );
                }
                Blend::Alpha => {
                    c3d::C3D_AlphaTest(false, sys::GPU_ALWAYS, 0);
                    c3d::C3D_AlphaBlend(
                        sys::GPU_BLEND_ADD,
                        sys::GPU_BLEND_ADD,
                        sys::GPU_SRC_ALPHA,
                        sys::GPU_ONE_MINUS_SRC_ALPHA,
                        sys::GPU_SRC_ALPHA,
                        sys::GPU_ONE_MINUS_SRC_ALPHA,
                    );
                }
                Blend::Add => {
                    // Cut-out texels still dropped (the sun's corners).
                    c3d::C3D_AlphaTest(true, sys::GPU_GREATER, 127);
                    c3d::C3D_AlphaBlend(
                        sys::GPU_BLEND_ADD,
                        sys::GPU_BLEND_ADD,
                        sys::GPU_ONE,
                        sys::GPU_ONE,
                        sys::GPU_ZERO,
                        sys::GPU_ONE,
                    );
                }
            }
        }
    }

    /// Night vision (`BrightenTexture`: every texel becomes `(G/8, G, G/8)`)
    /// for world drawing; the sky and sun are drawn without it.
    pub fn set_night(&mut self, on: bool) {
        if self.night != on {
            self.night = on;
            self.texenv(self.tex_alpha);
        }
    }

    /// Colour = texture x vertex light. Alpha = the texture's, times
    /// `alpha` (0..255) if given. Stages: 0 texture (night vision: its
    /// green times (1/8, 1, 1/8)), 1 times the light, 2 fog.
    pub fn texenv(&mut self, alpha: Option<u8>) {
        self.tex_alpha = alpha;
        if self.prog != Prog::World {
            // Applied when the world program is next used.
            return;
        }
        unsafe {
            let a = u32::from(alpha.unwrap_or(255));
            let env = c3d::C3D_GetTexEnv(0);
            c3d::C3D_TexEnvInit(env);
            if self.night {
                c3d::C3D_TexEnvSrc(env, c3d::C3D_RGB, sys::GPU_TEXTURE0, sys::GPU_CONSTANT, 0);
                c3d::C3D_TexEnvOpRgb(
                    env,
                    sys::GPU_TEVOP_RGB_SRC_G,
                    sys::GPU_TEVOP_RGB_SRC_COLOR,
                    sys::GPU_TEVOP_RGB_SRC_COLOR,
                );
                c3d::C3D_TexEnvFunc(env, c3d::C3D_RGB, sys::GPU_MODULATE);
            } else {
                c3d::C3D_TexEnvSrc(env, c3d::C3D_RGB, sys::GPU_TEXTURE0, 0, 0);
                c3d::C3D_TexEnvFunc(env, c3d::C3D_RGB, sys::GPU_REPLACE);
            }
            c3d::C3D_TexEnvSrc(env, c3d::C3D_Alpha, sys::GPU_TEXTURE0, sys::GPU_CONSTANT, 0);
            c3d::C3D_TexEnvFunc(env, c3d::C3D_Alpha, sys::GPU_MODULATE);
            c3d::C3D_TexEnvColor(env, a << 24 | 32 << 16 | 255 << 8 | 32);
            // Stage 1: times the vertex light.
            let env = c3d::C3D_GetTexEnv(1);
            c3d::C3D_TexEnvInit(env);
            c3d::C3D_TexEnvSrc(
                env,
                c3d::C3D_RGB,
                sys::GPU_PREVIOUS,
                sys::GPU_PRIMARY_COLOR,
                0,
            );
            c3d::C3D_TexEnvFunc(env, c3d::C3D_RGB, sys::GPU_MODULATE);
            c3d::C3D_TexEnvSrc(env, c3d::C3D_Alpha, sys::GPU_PREVIOUS, 0, 0);
            c3d::C3D_TexEnvFunc(env, c3d::C3D_Alpha, sys::GPU_REPLACE);
            // Stage 2: fog colour mixed in by the vertex colour's alpha.
            let env = c3d::C3D_GetTexEnv(2);
            c3d::C3D_TexEnvInit(env);
            c3d::C3D_TexEnvSrc(
                env,
                c3d::C3D_RGB,
                sys::GPU_CONSTANT,
                sys::GPU_PREVIOUS,
                sys::GPU_PRIMARY_COLOR,
            );
            c3d::C3D_TexEnvOpRgb(
                env,
                sys::GPU_TEVOP_RGB_SRC_COLOR,
                sys::GPU_TEVOP_RGB_SRC_COLOR,
                sys::GPU_TEVOP_RGB_SRC_ALPHA,
            );
            c3d::C3D_TexEnvFunc(env, c3d::C3D_RGB, sys::GPU_INTERPOLATE);
            c3d::C3D_TexEnvSrc(env, c3d::C3D_Alpha, sys::GPU_PREVIOUS, 0, 0);
            c3d::C3D_TexEnvFunc(env, c3d::C3D_Alpha, sys::GPU_REPLACE);
            c3d::C3D_TexEnvColor(env, self.fog_rgb);
        }
    }

    fn set_attrs(&mut self, lit: Lit) {
        if self.prog != Prog::World {
            self.bind(Prog::World);
            self.texenv(self.tex_alpha);
        }
        if self.lit == Some(lit) {
            return;
        }
        unsafe {
            let ai = c3d::C3D_GetAttrInfo();
            c3d::AttrInfo_Init(ai);
            c3d::AttrInfo_AddLoader(ai, 0, sys::GPU_FLOAT, 3);
            c3d::AttrInfo_AddLoader(ai, 1, sys::GPU_FLOAT, 2);
            let f = match lit {
                Lit::Unsigned => sys::GPU_UNSIGNED_BYTE,
                Lit::Signed => sys::GPU_BYTE,
            };
            c3d::AttrInfo_AddLoader(ai, 2, f, 4);
        }
        self.lit = Some(lit);
    }

    /// Draws `count` vertices (triangles) of `buf` from `first`.
    pub fn draw(&mut self, buf: &LinearBuf<Vertex>, lit: Lit, first: usize, count: usize) {
        if count == 0 || first + count > buf.len() {
            return;
        }
        self.set_attrs(lit);
        unsafe {
            let bi = c3d::C3D_GetBufInfo();
            c3d::BufInfo_Init(bi);
            c3d::BufInfo_Add(
                bi,
                buf.ptr as *const c_void,
                size_of::<Vertex>() as isize,
                3,
                0x210,
            );
            c3d::C3D_DrawArrays(sys::GPU_TRIANGLES, first as i32, count as i32);
        }
    }

    /// Draws `count` vertices (triangles) of a flat-coloured buffer, blended
    /// by their alpha. The world texenv comes back on the next [`draw`].
    ///
    /// [`draw`]: Self::draw
    pub fn draw_flat(&mut self, buf: &LinearBuf<FlatVertex>, first: usize, count: usize) {
        if count == 0 || first + count > buf.len() {
            return;
        }
        if self.prog != Prog::Flat {
            self.bind(Prog::Flat);
            unsafe {
                let ai = c3d::C3D_GetAttrInfo();
                c3d::AttrInfo_Init(ai);
                c3d::AttrInfo_AddLoader(ai, 0, sys::GPU_FLOAT, 3);
                c3d::AttrInfo_AddLoader(ai, 1, sys::GPU_UNSIGNED_BYTE, 4);
                let env = c3d::C3D_GetTexEnv(0);
                c3d::C3D_TexEnvInit(env);
                c3d::C3D_TexEnvSrc(env, c3d::C3D_Both, sys::GPU_PRIMARY_COLOR, 0, 0);
                c3d::C3D_TexEnvFunc(env, c3d::C3D_Both, sys::GPU_REPLACE);
                c3d::C3D_TexEnvInit(c3d::C3D_GetTexEnv(1));
                c3d::C3D_TexEnvInit(c3d::C3D_GetTexEnv(2));
            }
        }
        unsafe {
            let bi = c3d::C3D_GetBufInfo();
            c3d::BufInfo_Init(bi);
            c3d::BufInfo_Add(
                bi,
                buf.ptr as *const c_void,
                size_of::<FlatVertex>() as isize,
                2,
                0x10,
            );
            c3d::C3D_DrawArrays(sys::GPU_TRIANGLES, first as i32, count as i32);
        }
    }

    /// Fraction of the GPU command buffer used this frame.
    pub fn cmdbuf_usage(&self) -> f32 {
        unsafe { c3d::C3D_GetCmdBufUsage() }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            c3d::C3D_RenderTargetDelete(self.target);
            c3d::C3D_RenderTargetDelete(self.bottom);
            sys::gfxSetDoubleBuffering(sys::GFX_BOTTOM, false);
            let _ = sys::shaderProgramFree(&mut *self.world);
            let _ = sys::shaderProgramFree(&mut *self.flat);
            sys::DVLB_Free(self._dvlb);
            c3d::C3D_Fini();
        }
    }
}
