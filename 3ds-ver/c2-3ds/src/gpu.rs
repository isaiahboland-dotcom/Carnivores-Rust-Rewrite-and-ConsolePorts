//! A thin layer over citro3d's C API for what the game draws: one shader,
//! textures from the cooked pack, vertex buffers in GPU-visible memory, and
//! the few render states the original uses.

use std::ffi::{c_void, CString};
use std::mem::{size_of, zeroed};

use c2_formats::cooked::CookedTexture;
use citro3d_sys as c3d;
use ctru_sys as sys;
use glam::Mat4;

static SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/world.shbin"));

/// 24 bytes: position, texture coordinate, 4 light bytes (see the shader).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub lit: [u8; 4],
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
        let n = data.len().min(self.cap);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr, n);
            let _ =
                sys::GSPGPU_FlushDataCache(self.ptr as *const c_void, (n * size_of::<T>()) as u32);
        }
        self.len = n;
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
    program: Box<sys::shaderProgram_s>,
    _dvlb: *mut sys::DVLB_s,
    _code: Vec<u32>,
    u_proj: i32,
    u_mv: i32,
    u_light: i32,
    u_lsel: i32,
    u_fog: i32,
    fog_rgb: u32,
    lit: Option<Lit>,
}

pub const TOP_W: usize = 400;
pub const TOP_H: usize = 240;

const fn transfer_flags() -> u32 {
    c3d::GX_TRANSFER_FLIP_VERT(false)
        | c3d::GX_TRANSFER_OUT_TILED(false)
        | c3d::GX_TRANSFER_RAW_COPY(false)
        | c3d::GX_TRANSFER_IN_FORMAT(sys::GX_TRANSFER_FMT_RGBA8)
        | c3d::GX_TRANSFER_OUT_FORMAT(sys::GX_TRANSFER_FMT_RGB8)
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
            c3d::C3D_RenderTargetSetOutput(target, sys::GFX_TOP, sys::GFX_LEFT, transfer_flags());

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
            let mut program: Box<sys::shaderProgram_s> = Box::new(zeroed());
            let _ = sys::shaderProgramInit(&mut *program);
            let _ = sys::shaderProgramSetVsh(&mut *program, (*dvlb).DVLE);
            c3d::C3D_BindProgram(&mut *program);
            let loc = |name: &str| {
                let c = CString::new(name).unwrap_or_default();
                i32::from(sys::shaderInstanceGetUniformLocation(
                    program.vertexShader,
                    c.as_ptr(),
                ))
            };
            let gpu = Self {
                target,
                u_proj: loc("projection"),
                u_mv: loc("modelView"),
                u_light: loc("light"),
                u_lsel: loc("lsel"),
                u_fog: loc("fog"),
                program,
                _dvlb: dvlb,
                _code: code,
                fog_rgb: 0,
                lit: None,
            };
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
            c3d::C3D_BindProgram(&mut *self.program);
            c3d::C3D_CullFace(sys::GPU_CULL_NONE);
        }
        self.lit = None;
        self.depth(true, false);
        self.blend(Blend::Cutout);
        self.texenv(None);
    }

    pub fn end(&mut self) {
        unsafe { c3d::C3D_FrameEnd(0) };
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
            c3d::C3D_FVUnifMtx4x4(sys::GPU_VERTEX_SHADER, self.u_proj, &p);
        }
    }

    pub fn set_model_view(&mut self, mv: &Mat4) {
        let m = c3d_mtx(mv);
        unsafe { c3d::C3D_FVUnifMtx4x4(sys::GPU_VERTEX_SHADER, self.u_mv, &m) };
    }

    /// Base light (0..1) and which light byte to add (0..=3), if any.
    pub fn set_light(&mut self, base: f32, select: Option<usize>) {
        let mut s = [0.0f32; 4];
        if let Some(i) = select {
            s[i & 3] = 1.0 / 255.0;
        }
        unsafe {
            c3d::C3D_FVUnifSet(sys::GPU_VERTEX_SHADER, self.u_light, base, 0.0, 0.0, 0.0);
            c3d::C3D_FVUnifSet(sys::GPU_VERTEX_SHADER, self.u_lsel, s[0], s[1], s[2], s[3]);
        }
    }

    /// Linear fog from `start` to `end` (world units from the eye) to
    /// `rgb`. The GPU's own fog table works on depth-buffer values, which
    /// are too non-linear at this view distance, so the shader computes it.
    pub fn set_fog(&mut self, rgb: [u8; 3], start: f32, end: f32) {
        let k = 1.0 / (end - start).max(1.0);
        unsafe { c3d::C3D_FVUnifSet(sys::GPU_VERTEX_SHADER, self.u_fog, k, -start * k, 0.0, 0.0) };
        let [r, g, b] = rgb.map(u32::from);
        self.fog_rgb = 0xff00_0000 | b << 16 | g << 8 | r;
    }

    /// No fog (the view model).
    pub fn no_fog(&mut self) {
        unsafe { c3d::C3D_FVUnifSet(sys::GPU_VERTEX_SHADER, self.u_fog, 0.0, 0.0, 0.0, 0.0) };
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
            }
        }
    }

    /// Colour = texture x vertex light. Alpha = the texture's, times
    /// `alpha` (0..255) if given.
    pub fn texenv(&mut self, alpha: Option<u8>) {
        unsafe {
            let env = c3d::C3D_GetTexEnv(0);
            c3d::C3D_TexEnvInit(env);
            c3d::C3D_TexEnvSrc(
                env,
                c3d::C3D_RGB,
                sys::GPU_TEXTURE0,
                sys::GPU_PRIMARY_COLOR,
                0,
            );
            c3d::C3D_TexEnvFunc(env, c3d::C3D_RGB, sys::GPU_MODULATE);
            match alpha {
                None => {
                    c3d::C3D_TexEnvSrc(env, c3d::C3D_Alpha, sys::GPU_TEXTURE0, 0, 0);
                    c3d::C3D_TexEnvFunc(env, c3d::C3D_Alpha, sys::GPU_REPLACE);
                }
                Some(a) => {
                    c3d::C3D_TexEnvSrc(
                        env,
                        c3d::C3D_Alpha,
                        sys::GPU_TEXTURE0,
                        sys::GPU_CONSTANT,
                        0,
                    );
                    c3d::C3D_TexEnvFunc(env, c3d::C3D_Alpha, sys::GPU_MODULATE);
                    c3d::C3D_TexEnvColor(env, u32::from(a) << 24);
                }
            }
            // Stage 1: fog colour mixed in by the vertex colour's alpha.
            let env = c3d::C3D_GetTexEnv(1);
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

    /// Fraction of the GPU command buffer used this frame.
    pub fn cmdbuf_usage(&self) -> f32 {
        unsafe { c3d::C3D_GetCmdBufUsage() }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            c3d::C3D_RenderTargetDelete(self.target);
            let _ = sys::shaderProgramFree(&mut *self.program);
            sys::DVLB_Free(self._dvlb);
            c3d::C3D_Fini();
        }
    }
}
