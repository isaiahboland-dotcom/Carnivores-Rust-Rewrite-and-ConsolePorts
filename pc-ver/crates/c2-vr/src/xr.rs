//! The headset: an OpenXR session drawn with Vulkan (`XR_KHR_vulkan_enable2`,
//! after the `openxr` crate's `vulkan` example), and the frame loop that
//! hands each frame to a [`Scene`] (the test scene, an area).
//!
//! The runtime picks the GPU and creates the Vulkan instance and device;
//! each eye is drawn into its own layer of one swapchain. Head and
//! controller poses are read every frame in the stage space (floor at 0)
//! or, where the runtime has none (Monado's PS VR2 driver), the local space,
//! its floor put `EYE_HEIGHT_M` below the head's first pose; likewise a
//! stage floor that puts the head implausibly low. Quitting: the runtime's own quit (SteamVR dashboard), the left
//! controller's menu button, or the time limit.

#![allow(unsafe_code)] // Vulkan and OpenXR handles cross the FFI boundary here.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use ash::vk::{self, Handle};
use glam::{Quat, Vec2, Vec3};
use openxr as xr;

use crate::gpu::Gpu;
use crate::scene::{Eye, Frame, Input, MakeScene, Scene};

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
const VIEWS: usize = 2;
/// Frames in flight (per-frame buffers in the scenes).
pub const DEPTH: usize = 2;
/// A stage floor less than this below the head is taken as wrong.
const MIN_HEAD_M: f32 = 0.6;

/// What a run measured, printed at the end.
pub struct Report {
    pub runtime: String,
    pub system: String,
    pub resolution: (u32, u32),
    pub frames: u64,
    pub skipped: u64,
    pub seconds: f32,
    /// The slowest frame's CPU time (waiting for the frame excluded), ms.
    pub worst_cpu_ms: f32,
}

/// Swapchain formats wanted, best first (sRGB: the scenes write linear
/// colour).
const FORMATS: [vk::Format; 4] = [
    vk::Format::R8G8B8A8_SRGB,
    vk::Format::B8G8R8A8_SRGB,
    vk::Format::R8G8B8A8_UNORM,
    vk::Format::B8G8R8A8_UNORM,
];

/// A text for errors from the runtime (`xr::sys::Result`).
fn xr_err(what: &'static str) -> impl FnOnce(xr::sys::Result) -> anyhow::Error {
    move |e| anyhow!("{what}: {e}")
}

struct Actions {
    set: xr::ActionSet,
    left: xr::Action<xr::Posef>,
    right: xr::Action<xr::Posef>,
    quit: xr::Action<bool>,
    walk: xr::Action<xr::Vector2f>,
    turn: xr::Action<xr::Vector2f>,
}

impl Actions {
    fn new(instance: &xr::Instance) -> Result<Self> {
        let set = instance
            .create_action_set("hunt", "Hunt", 0)
            .map_err(xr_err("creating actions"))?;
        let e = xr_err("creating actions");
        let a = Self {
            left: set
                .create_action("left_hand", "Left hand", &[])
                .map_err(e)?,
            right: set
                .create_action("right_hand", "Right hand", &[])
                .map_err(xr_err("creating actions"))?,
            quit: set
                .create_action("quit", "Quit", &[])
                .map_err(xr_err("creating actions"))?,
            walk: set
                .create_action("walk", "Walk", &[])
                .map_err(xr_err("creating actions"))?,
            turn: set
                .create_action("turn", "Turn", &[])
                .map_err(xr_err("creating actions"))?,
            set,
        };
        a.suggest(instance);
        Ok(a)
    }

    /// A pose per hand, the quit button and the sticks, for each controller
    /// kind OpenXR names (the runtime takes the ones it has).
    fn suggest(&self, instance: &xr::Instance) {
        // (profile, quit button, stick or trackpad)
        let profiles: [(&str, Option<&str>, Option<&str>); 5] = [
            (
                "/interaction_profiles/khr/simple_controller",
                Some("/user/hand/left/input/menu/click"),
                None,
            ),
            (
                "/interaction_profiles/valve/index_controller",
                Some("/user/hand/left/input/b/click"),
                Some("thumbstick"),
            ),
            (
                "/interaction_profiles/oculus/touch_controller",
                Some("/user/hand/left/input/menu/click"),
                Some("thumbstick"),
            ),
            (
                "/interaction_profiles/htc/vive_controller",
                Some("/user/hand/left/input/menu/click"),
                Some("trackpad"),
            ),
            (
                "/interaction_profiles/microsoft/motion_controller",
                None,
                Some("thumbstick"),
            ),
        ];
        for (profile, quit, stick) in profiles {
            let path = |p: &str| instance.string_to_path(p);
            let r = (|| -> std::result::Result<(), xr::sys::Result> {
                let mut b = vec![
                    xr::Binding::new(&self.left, path("/user/hand/left/input/grip/pose")?),
                    xr::Binding::new(&self.right, path("/user/hand/right/input/grip/pose")?),
                ];
                if let Some(q) = quit {
                    b.push(xr::Binding::new(&self.quit, path(q)?));
                }
                if let Some(s) = stick {
                    b.push(xr::Binding::new(
                        &self.walk,
                        path(&format!("/user/hand/left/input/{s}"))?,
                    ));
                    b.push(xr::Binding::new(
                        &self.turn,
                        path(&format!("/user/hand/right/input/{s}"))?,
                    ));
                }
                instance.suggest_interaction_profile_bindings(path(profile)?, &b)
            })();
            if let Err(e) = r {
                println!("  controller bindings for {profile}: {e}");
            }
        }
    }
}

fn eye(v: &xr::View) -> Eye {
    let (p, o, f) = (v.pose.position, v.pose.orientation, v.fov);
    Eye {
        pos: Vec3::new(p.x, p.y, p.z),
        rot: Quat::from_xyzw(o.x, o.y, o.z, o.w),
        tan: [
            f.angle_left.tan(),
            f.angle_right.tan(),
            f.angle_up.tan(),
            f.angle_down.tan(),
        ],
    }
}

/// Runs the scene `make` builds until the runtime or the player quits, or
/// for `limit` (tests, and headsets without controllers).
pub fn run(make: MakeScene, limit: Option<Duration>) -> Result<Report> {
    // SAFETY: the system's OpenXR loader, which the spec defines.
    let entry = unsafe { xr::Entry::load(&()) }.map_err(|e| {
        anyhow!("the OpenXR loader (libopenxr_loader / openxr_loader.dll) didn't load: {e}")
    })?;
    let available = entry
        .enumerate_extensions()
        .map_err(xr_err("listing OpenXR extensions"))?;
    if !available.khr_vulkan_enable2 {
        bail!("this OpenXR runtime can't draw with Vulkan (no XR_KHR_vulkan_enable2)");
    }
    let mut exts = xr::ExtensionSet::default();
    exts.khr_vulkan_enable2 = true;
    let instance = entry
        .create_instance(
            &xr::ApplicationInfo {
                application_name: "Carnivores 2 (Rust) VR",
                application_version: 0,
                engine_name: "c2rs",
                engine_version: 0,
                api_version: xr::Version::new(1, 0, 0),
            },
            &exts,
            &[],
            &(),
        )
        .map_err(xr_err("starting OpenXR (is SteamVR or Monado running?)"))?;
    let props = instance
        .properties()
        .map_err(xr_err("reading the runtime"))?;
    let runtime = format!("{} {}", props.runtime_name, props.runtime_version);
    println!("OpenXR runtime: {runtime}");
    let system = instance
        .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .map_err(xr_err("finding the headset (is it connected and on?)"))?;
    let system_name = instance
        .system_properties(system)
        .map(|p| p.system_name)
        .unwrap_or_default();
    println!("Headset: {system_name}");
    let blend = *instance
        .enumerate_environment_blend_modes(system, VIEW_TYPE)
        .map_err(xr_err("reading blend modes"))?
        .first()
        .ok_or_else(|| anyhow!("the runtime offers no blend mode"))?;
    let reqs = instance
        .graphics_requirements::<xr::Vulkan>(system)
        .map_err(xr_err("reading the runtime's Vulkan needs"))?;
    if xr::Version::new(1, 1, 0) < reqs.min_api_version_supported {
        bail!(
            "the runtime needs Vulkan {} or newer",
            reqs.min_api_version_supported
        );
    }

    // SAFETY: from here on, Vulkan objects are made by the runtime for this
    // instance and destroyed below, after every OpenXR handle using them.
    unsafe {
        let vk_entry = ash::Entry::load().context("loading Vulkan")?;
        let get_proc = std::mem::transmute::<
            vk::PFN_vkGetInstanceProcAddr,
            xr::sys::platform::VkGetInstanceProcAddr,
        >(vk_entry.static_fn().get_instance_proc_addr);
        let app = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
        let raw = instance
            .create_vulkan_instance(
                system,
                get_proc,
                &vk::InstanceCreateInfo::default().application_info(&app) as *const _ as *const _,
            )
            .map_err(xr_err("creating the Vulkan instance"))?
            .map_err(|e| anyhow!("Vulkan instance: {}", vk::Result::from_raw(e)))?;
        let vk_instance =
            ash::Instance::load(vk_entry.static_fn(), vk::Instance::from_raw(raw as _));
        let physical = vk::PhysicalDevice::from_raw(
            instance
                .vulkan_graphics_device(system, vk_instance.handle().as_raw() as _)
                .map_err(xr_err("finding the headset's GPU"))? as _,
        );
        println!(
            "GPU: {}",
            vk_instance
                .get_physical_device_properties(physical)
                .device_name_as_c_str()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        let family = vk_instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or_else(|| anyhow!("the GPU has no graphics queue"))? as u32;
        let raw = instance
            .create_vulkan_device(
                system,
                get_proc,
                physical.as_raw() as _,
                &vk::DeviceCreateInfo::default().queue_create_infos(&[
                    vk::DeviceQueueCreateInfo::default()
                        .queue_family_index(family)
                        .queue_priorities(&[1.0]),
                ]) as *const _ as *const _,
            )
            .map_err(xr_err("creating the Vulkan device"))?
            .map_err(|e| anyhow!("Vulkan device: {}", vk::Result::from_raw(e)))?;
        let device = ash::Device::load(vk_instance.fp_v1_0(), vk::Device::from_raw(raw as _));
        let gpu = Gpu::new(vk_instance, device, physical, family);

        let result = session(&instance, system, blend, &gpu, make, limit);
        let _ = gpu.device.device_wait_idle();
        gpu.device.destroy_device(None);
        gpu.instance.destroy_instance(None);
        let (frames, skipped, seconds, worst_cpu_ms, resolution) = result?;
        Ok(Report {
            runtime,
            system: system_name,
            resolution,
            frames,
            skipped,
            seconds,
            worst_cpu_ms,
        })
    }
}

type Measured = (u64, u64, f32, f32, (u32, u32));

/// The session on `gpu`: everything that must go before the device does.
fn session(
    instance: &xr::Instance,
    system: xr::SystemId,
    blend: xr::EnvironmentBlendMode,
    gpu: &Gpu,
    make: MakeScene,
    limit: Option<Duration>,
) -> Result<Measured> {
    // SAFETY: the handles are the runtime's own, for this system.
    let (session, mut frame_wait, mut frame_stream) = unsafe {
        instance.create_session::<xr::Vulkan>(
            system,
            &xr::vulkan::SessionCreateInfo {
                instance: gpu.instance.handle().as_raw() as _,
                physical_device: gpu.physical.as_raw() as _,
                device: gpu.device.handle().as_raw() as _,
                queue_family_index: gpu.family,
                queue_index: 0,
            },
        )
    }
    .map_err(xr_err("starting the VR session"))?;
    let formats = session
        .enumerate_swapchain_formats()
        .map_err(xr_err("reading swapchain formats"))?;
    let format = FORMATS
        .into_iter()
        .find(|f| formats.contains(&(f.as_raw() as u32)))
        .ok_or_else(|| anyhow!("the runtime offers no 8-bit colour format"))?;

    let actions = Actions::new(instance)?;
    session
        .attach_action_sets(&[&actions.set])
        .map_err(xr_err("attaching actions"))?;
    let hand_spaces = [&actions.left, &actions.right]
        .map(|a| a.create_space(&session, xr::Path::NULL, xr::Posef::IDENTITY));
    let [Ok(left_space), Ok(right_space)] = hand_spaces else {
        bail!("creating hand spaces failed");
    };
    let stage_ok = session
        .enumerate_reference_spaces()
        .map(|s| s.contains(&xr::ReferenceSpaceType::STAGE))
        .unwrap_or(false);
    let (space_type, floor) = if stage_ok {
        (xr::ReferenceSpaceType::STAGE, Some(0.0))
    } else {
        println!(
            "No room-scale (stage) space: where your head starts is taken as standing eye height."
        );
        (xr::ReferenceSpaceType::LOCAL, None)
    };
    let space = session
        .create_reference_space(space_type, xr::Posef::IDENTITY)
        .map_err(xr_err("creating the tracking space"))?;

    let views = instance
        .enumerate_view_configuration_views(system, VIEW_TYPE)
        .map_err(xr_err("reading the eye views"))?;
    if views.len() != VIEWS
        || views[0].recommended_image_rect_width != views[1].recommended_image_rect_width
    {
        bail!("the headset doesn't have two same-sized eye views");
    }
    let extent = vk::Extent2D {
        width: views[0].recommended_image_rect_width,
        height: views[0].recommended_image_rect_height,
    };
    println!("Per eye: {} x {}", extent.width, extent.height);
    let mut swapchain = session
        .create_swapchain(&xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT,
            format: format.as_raw() as _,
            sample_count: 1,
            width: extent.width,
            height: extent.height,
            face_count: 1,
            array_size: VIEWS as u32,
            mip_count: 1,
        })
        .map_err(xr_err("creating the swapchain"))?;

    let mut scene = make(gpu, format)?;
    let mut targets = Targets::default();
    let looped = (|| -> Result<Measured> {
        let extra = scene.eye_targets(gpu, extent)?;
        for raw in swapchain
            .enumerate_images()
            .map_err(xr_err("reading swapchain images"))?
        {
            targets.add(
                gpu,
                scene.as_ref(),
                vk::Image::from_raw(raw),
                format,
                extent,
                &extra,
            )?;
        }
        frame_loop(FrameLoop {
            instance,
            session: &session,
            frame_wait: &mut frame_wait,
            frame_stream: &mut frame_stream,
            swapchain: &mut swapchain,
            actions: &actions,
            hands: [&left_space, &right_space],
            space: &space,
            blend,
            floor,
            extent,
            gpu,
            scene: scene.as_mut(),
            targets: &targets,
            limit,
        })
    })();
    // OpenXR first (it may still use the Vulkan objects), then ours.
    drop((
        swapchain,
        space,
        left_space,
        right_space,
        actions,
        frame_stream,
        frame_wait,
        session,
    ));
    // SAFETY: the session is gone; nothing else uses the GPU.
    unsafe {
        let _ = gpu.device.device_wait_idle();
    }
    targets.destroy(gpu);
    scene.destroy(gpu);
    looped
}

/// Per swapchain image, per eye: the colour layer's view and framebuffer.
#[derive(Default)]
struct Targets {
    views: Vec<vk::ImageView>,
    framebuffers: Vec<[vk::Framebuffer; 2]>,
}

impl Targets {
    fn add(
        &mut self,
        gpu: &Gpu,
        scene: &dyn Scene,
        image: vk::Image,
        format: vk::Format,
        extent: vk::Extent2D,
        extra: &[Vec<vk::ImageView>; 2],
    ) -> Result<()> {
        let mut fbs = [vk::Framebuffer::null(); 2];
        for (layer, fb) in fbs.iter_mut().enumerate() {
            // SAFETY: views and framebuffers of the runtime's image, kept
            // until `destroy`.
            unsafe {
                let view = gpu.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: layer as u32,
                            layer_count: 1,
                        }),
                    None,
                )?;
                self.views.push(view);
                let mut attachments = vec![view];
                attachments.extend(&extra[layer]);
                *fb = gpu.device.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(scene.render_pass())
                        .width(extent.width)
                        .height(extent.height)
                        .attachments(&attachments)
                        .layers(1),
                    None,
                )?;
            }
        }
        self.framebuffers.push(fbs);
        Ok(())
    }

    fn destroy(&mut self, gpu: &Gpu) {
        // SAFETY: the GPU is idle.
        unsafe {
            for fb in self.framebuffers.drain(..).flatten() {
                gpu.device.destroy_framebuffer(fb, None);
            }
            for v in self.views.drain(..) {
                gpu.device.destroy_image_view(v, None);
            }
        }
    }
}

struct FrameLoop<'a> {
    instance: &'a xr::Instance,
    session: &'a xr::Session<xr::Vulkan>,
    frame_wait: &'a mut xr::FrameWaiter,
    frame_stream: &'a mut xr::FrameStream<xr::Vulkan>,
    swapchain: &'a mut xr::Swapchain<xr::Vulkan>,
    actions: &'a Actions,
    hands: [&'a xr::Space; 2],
    space: &'a xr::Space,
    blend: xr::EnvironmentBlendMode,
    /// The tracking space's floor; `None`: set from the first head pose.
    floor: Option<f32>,
    extent: vk::Extent2D,
    gpu: &'a Gpu,
    scene: &'a mut dyn Scene,
    targets: &'a Targets,
    limit: Option<Duration>,
}

fn frame_loop(l: FrameLoop<'_>) -> Result<Measured> {
    let gpu = l.gpu;
    let dev = &gpu.device;
    // SAFETY: a pool, buffers and fences on our device, freed at the end.
    let (pool, cmds, fences) = unsafe {
        let pool = dev.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(gpu.family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        let cmds = dev.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .command_buffer_count(DEPTH as u32),
        )?;
        let mut fences = Vec::new();
        for _ in 0..DEPTH {
            fences.push(dev.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?);
        }
        (pool, cmds, fences)
    };

    let start = Instant::now();
    let (mut frames, mut skipped, mut worst_cpu) = (0u64, 0u64, 0f32);
    let mut events = xr::EventDataBuffer::new();
    let (mut running, mut exiting) = (false, false);
    let mut slot = 0usize;
    let mut hands_seen = [false; 2];
    let mut turn_held = false;
    // A gamepad walks and turns too (headsets without controllers: the PS
    // VR2 under Monado); its - / Select quits.
    let mut pads = c2_pad::Pads::new();
    for name in pads.names() {
        println!("Gamepad: {name} (left stick walks, right stick turns, - / Select quits)");
    }
    let mut floor = l.floor;
    let mut head_reported = false;
    let mut last = Instant::now();
    let r = (|| -> Result<()> {
        'frames: loop {
            while let Some(ev) = l
                .instance
                .poll_event(&mut events)
                .map_err(xr_err("reading runtime events"))?
            {
                match ev {
                    xr::Event::SessionStateChanged(e) => match e.state() {
                        xr::SessionState::READY => {
                            l.session
                                .begin(VIEW_TYPE)
                                .map_err(xr_err("beginning the session"))?;
                            running = true;
                            println!("In the headset now. Left menu button (Index: B) quits.");
                        }
                        xr::SessionState::STOPPING => {
                            l.session.end().map_err(xr_err("ending the session"))?;
                            running = false;
                        }
                        xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => {
                            break 'frames;
                        }
                        _ => {}
                    },
                    xr::Event::InstanceLossPending(_) => break 'frames,
                    _ => {}
                }
            }
            let timed_out = l.limit.is_some_and(|t| start.elapsed() >= t);
            if !running {
                if timed_out {
                    break 'frames;
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            let state = l.frame_wait.wait().map_err(xr_err("waiting for a frame"))?;
            let cpu = Instant::now();
            let dt = last.elapsed().as_secs_f32().min(0.1);
            last = Instant::now();
            l.frame_stream
                .begin()
                .map_err(xr_err("beginning a frame"))?;
            if !state.should_render {
                skipped += 1;
                l.frame_stream
                    .end(state.predicted_display_time, l.blend, &[])
                    .map_err(xr_err("ending a frame"))?;
                continue;
            }

            let a = l.actions;
            l.session
                .sync_actions(&[(&a.set).into()])
                .map_err(xr_err("reading controllers"))?;
            let pressed = a
                .quit
                .state(l.session, xr::Path::NULL)
                .is_ok_and(|s| s.current_state && s.changed_since_last_sync);
            let pad = pads.poll();
            let pressed = pressed || pad.pressed(c2_pad::Action::Exit);
            if !exiting && (pressed || timed_out) {
                exiting = true;
                let _ = l.session.request_exit();
            }
            let stick = |act: &xr::Action<xr::Vector2f>| {
                act.state(l.session, xr::Path::NULL)
                    .ok()
                    .filter(|s| s.is_active)
                    .map_or(Vec2::ZERO, |s| {
                        Vec2::new(s.current_state.x, s.current_state.y)
                    })
            };
            let turn = match stick(&a.turn).x {
                t if t != 0.0 => t,
                _ => pad.look[0],
            };
            let snap_turn = if turn.abs() > 0.6 && !turn_held {
                turn_held = true;
                turn.signum() as i32
            } else {
                if turn.abs() < 0.3 {
                    turn_held = false;
                }
                0
            };
            let input = Input {
                walk: match stick(&a.walk) {
                    w if w != Vec2::ZERO => w,
                    _ => Vec2::from(pad.walk),
                },
                snap_turn,
            };
            let mut hands = [None; 2];
            for (i, h) in l.hands.iter().enumerate() {
                if let Ok(loc) = h.locate(l.space, state.predicted_display_time) {
                    if loc
                        .location_flags
                        .contains(xr::SpaceLocationFlags::POSITION_VALID)
                    {
                        let p = loc.pose.position;
                        hands[i] = Some(Vec3::new(p.x, p.y, p.z));
                        if !hands_seen[i] {
                            hands_seen[i] = true;
                            println!("{} controller tracked", ["Left", "Right"][i]);
                        }
                    }
                }
            }
            let (_, views) = l
                .session
                .locate_views(VIEW_TYPE, state.predicted_display_time, l.space)
                .map_err(xr_err("locating the eyes"))?;
            if views.len() < VIEWS {
                bail!("the runtime located {} eyes", views.len());
            }
            let eyes = [eye(&views[0]), eye(&views[1])];
            let head_y = (eyes[0].pos.y + eyes[1].pos.y) * 0.5;
            if !head_reported {
                head_reported = true;
                println!("Head starts {head_y:.2} m up in the tracking space.");
                if floor.is_some_and(|f| head_y - f < MIN_HEAD_M) {
                    println!("  That's below a seated head: the floor is set from it instead.");
                    floor = None;
                }
            }
            let floor = *floor.get_or_insert(head_y - crate::scene::EYE_HEIGHT_M);

            let index = l
                .swapchain
                .acquire_image()
                .map_err(xr_err("taking a swapchain image"))? as usize;
            let framebuffers = *l
                .targets
                .framebuffers
                .get(index)
                .ok_or_else(|| anyhow!("swapchain image {index} unknown"))?;
            let cmd = cmds[slot];
            // SAFETY: this slot's fence guards its command buffer and the
            // scene's per-slot buffers.
            unsafe {
                dev.wait_for_fences(&[fences[slot]], true, u64::MAX)?;
                dev.reset_fences(&[fences[slot]])?;
                dev.begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
            }
            l.scene.record(
                gpu,
                &Frame {
                    cmd,
                    slot,
                    eyes,
                    hands,
                    floor,
                    extent: l.extent,
                    framebuffers,
                    dt,
                    input,
                },
            )?;
            l.swapchain
                .wait_image(xr::Duration::INFINITE)
                .map_err(xr_err("waiting for a swapchain image"))?;
            // SAFETY: the command buffer was recorded above.
            unsafe {
                dev.end_command_buffer(cmd)?;
                dev.queue_submit(
                    gpu.queue,
                    &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                    fences[slot],
                )?;
            }
            l.swapchain
                .release_image()
                .map_err(xr_err("handing over an image"))?;

            let rect = xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di {
                    width: l.extent.width as i32,
                    height: l.extent.height as i32,
                },
            };
            let swapchain = &*l.swapchain;
            let layer_views = [0, 1].map(|i| {
                xr::CompositionLayerProjectionView::new()
                    .pose(views[i].pose)
                    .fov(views[i].fov)
                    .sub_image(
                        xr::SwapchainSubImage::new()
                            .swapchain(swapchain)
                            .image_array_index(i as u32)
                            .image_rect(rect),
                    )
            });
            l.frame_stream
                .end(
                    state.predicted_display_time,
                    l.blend,
                    &[&xr::CompositionLayerProjection::new()
                        .space(l.space)
                        .views(&layer_views)],
                )
                .map_err(xr_err("showing a frame"))?;
            frames += 1;
            slot = (slot + 1) % DEPTH;
            worst_cpu = worst_cpu.max(cpu.elapsed().as_secs_f32() * 1000.0);
        }
        Ok(())
    })();
    let seconds = start.elapsed().as_secs_f32();
    // SAFETY: wait for our frames, then free what this loop made.
    unsafe {
        let _ = dev.wait_for_fences(&fences, true, u64::MAX);
        for f in fences {
            dev.destroy_fence(f, None);
        }
        dev.destroy_command_pool(pool, None);
    }
    r?;
    Ok((
        frames,
        skipped,
        seconds,
        worst_cpu,
        (l.extent.width, l.extent.height),
    ))
}
