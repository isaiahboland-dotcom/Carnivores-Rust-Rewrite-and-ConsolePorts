//! The Vulkan device the runtime made for us, and the few things the scenes
//! need from it: mapped buffers, device images, one-off uploads.

#![allow(unsafe_code)] // Raw Vulkan.

use anyhow::{anyhow, Result};
use ash::vk;

pub struct Gpu {
    pub instance: ash::Instance,
    pub device: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub queue: vk::Queue,
    pub family: u32,
    memory: vk::PhysicalDeviceMemoryProperties,
}

/// A host-visible, coherent buffer, mapped for its whole life.
pub struct Buffer {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    ptr: *mut u8,
    size: u64,
}

impl Buffer {
    pub fn size(&self) -> u64 {
        self.size
    }
    /// Copies `bytes` in at `offset` (cut to the buffer's size).
    pub fn write(&self, offset: u64, bytes: &[u8]) {
        if offset >= self.size || bytes.is_empty() {
            return;
        }
        let Ok(offset) = usize::try_from(offset) else {
            return;
        };
        let Ok(size) = usize::try_from(self.size) else {
            return;
        };
        let n = bytes.len().min(size - offset);
        // SAFETY: Buffer construction must establish a valid mapping of `size`
        // bytes. offset < size and n <= size - offset. Source and destination
        // must not overlap; safe callers cannot obtain the private mapped pointer.
        // Caller scheduling must prevent concurrent CPU writes/GPU access.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr.add(offset), n);
        }
    }
}

/// A device-local image and its memory.
pub struct Image {
    pub image: vk::Image,
    memory: vk::DeviceMemory,
}

/// A Vulkan object a [`Cleanup`] may have to destroy: a buffer or image
/// with its memory (freeing mapped memory unmaps it), or a plain handle.
#[derive(Clone, Copy)]
enum Made {
    Bound(Raw, vk::DeviceMemory),
    Handle(Raw),
}

/// A plain Vulkan handle [`Cleanup::add`] takes.
pub(crate) trait Handle: Copy {
    fn raw(self) -> Raw;
}

macro_rules! handles {
    ($($ty:ident => $destroy:ident),* $(,)?) => {
        #[derive(Clone, Copy)]
        pub(crate) enum Raw {
            $($ty(vk::$ty)),*
        }
        $(impl Handle for vk::$ty {
            fn raw(self) -> Raw {
                Raw::$ty(self)
            }
        })*
        impl Raw {
            /// # Safety
            /// As [`Cleanup::new`].
            unsafe fn destroy(self, dev: &ash::Device) {
                // SAFETY: the caller's.
                unsafe {
                    match self {
                        $(Raw::$ty(h) => dev.$destroy(h, None)),*
                    }
                }
            }
        }
    };
}

handles! {
    Buffer => destroy_buffer,
    Image => destroy_image,
    DeviceMemory => free_memory,
    ImageView => destroy_image_view,
    Sampler => destroy_sampler,
    ShaderModule => destroy_shader_module,
    RenderPass => destroy_render_pass,
    Framebuffer => destroy_framebuffer,
    Pipeline => destroy_pipeline,
    PipelineLayout => destroy_pipeline_layout,
    DescriptorSetLayout => destroy_descriptor_set_layout,
    DescriptorPool => destroy_descriptor_pool,
    CommandPool => destroy_command_pool,
    Fence => destroy_fence,
}

/// What a constructor has made so far, destroyed (newest first) when the
/// guard drops: on an early `?` return, or always for temporaries (shader
/// modules, staging). [`keep`](Cleanup::keep) hands everything to the caller.
pub(crate) struct Cleanup<'g> {
    gpu: &'g Gpu,
    made: Vec<Made>,
}

impl<'g> Cleanup<'g> {
    /// # Safety
    /// Everything added must belong to `gpu`'s device and be destroyed by
    /// nothing else unless [`keep`](Cleanup::keep) is called; when the guard
    /// drops, no GPU work may still use it.
    pub(crate) unsafe fn new(gpu: &'g Gpu) -> Self {
        Self {
            gpu,
            made: Vec::new(),
        }
    }

    pub(crate) fn add<H: Handle>(&mut self, h: H) -> H {
        self.made.push(Made::Handle(h.raw()));
        h
    }

    pub(crate) fn buffer(&mut self, b: Buffer) -> Buffer {
        self.made.push(Made::Bound(b.buffer.raw(), b.memory));
        b
    }

    pub(crate) fn image(&mut self, i: Image) -> Image {
        self.made.push(Made::Bound(i.image.raw(), i.memory));
        i
    }

    /// A texture's image and view (`Gpu::texture`).
    pub(crate) fn texture(&mut self, (i, v): (Image, vk::ImageView)) -> (Image, vk::ImageView) {
        let i = self.image(i);
        (i, self.add(v))
    }

    /// Everything added is the caller's to destroy now.
    pub(crate) fn keep(mut self) {
        self.made.clear();
    }
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let dev = &self.gpu.device;
        // SAFETY: `Cleanup::new`'s contract: ours, unused, destroyed once.
        unsafe {
            for m in self.made.drain(..).rev() {
                match m {
                    Made::Bound(h, memory) => {
                        h.destroy(dev);
                        dev.free_memory(memory, None);
                    }
                    Made::Handle(h) => h.destroy(dev),
                }
            }
        }
    }
}

impl Gpu {
    /// # Safety
    /// The handles must be live and belong together.
    pub unsafe fn new(
        instance: ash::Instance,
        device: ash::Device,
        physical: vk::PhysicalDevice,
        family: u32,
    ) -> Self {
        unsafe {
            let queue = device.get_device_queue(family, 0);
            let memory = instance.get_physical_device_memory_properties(physical);
            Self {
                instance,
                device,
                physical,
                queue,
                family,
                memory,
            }
        }
    }

    fn memory_type(&self, bits: u32, want: vk::MemoryPropertyFlags) -> Result<u32> {
        (0..self.memory.memory_type_count)
            .find(|&i| {
                bits & (1 << i) != 0
                    && self.memory.memory_types[i as usize]
                        .property_flags
                        .contains(want)
            })
            .ok_or_else(|| anyhow!("no GPU memory of kind {want:?}"))
    }

    pub fn buffer(&self, size: u64, usage: vk::BufferUsageFlags) -> Result<Buffer> {
        let size = size.max(16);
        // SAFETY: plain object creation on our device; `made` frees the
        // pieces, unused, if a later step fails.
        unsafe {
            let mut made = Cleanup::new(self);
            let buffer = made.add(
                self.device.create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(size)
                        .usage(usage)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE),
                    None,
                )?,
            );
            let req = self.device.get_buffer_memory_requirements(buffer);
            let kind = self.memory_type(
                req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let memory = made.add(
                self.device.allocate_memory(
                    &vk::MemoryAllocateInfo::default()
                        .allocation_size(req.size)
                        .memory_type_index(kind),
                    None,
                )?,
            );
            self.device.bind_buffer_memory(buffer, memory, 0)?;
            let ptr = self
                .device
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?
                .cast::<u8>();
            made.keep();
            Ok(Buffer {
                buffer,
                memory,
                ptr,
                size,
            })
        }
    }

    pub fn buffer_with(&self, bytes: &[u8], usage: vk::BufferUsageFlags) -> Result<Buffer> {
        let b = self.buffer(bytes.len() as u64, usage)?;
        b.write(0, bytes);
        Ok(b)
    }

    pub fn image(&self, info: &vk::ImageCreateInfo) -> Result<Image> {
        // SAFETY: plain object creation on our device; `made` frees the
        // pieces, unused, if a later step fails.
        unsafe {
            let mut made = Cleanup::new(self);
            let image = made.add(self.device.create_image(info, None)?);
            let req = self.device.get_image_memory_requirements(image);
            let kind =
                self.memory_type(req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
            let memory = made.add(
                self.device.allocate_memory(
                    &vk::MemoryAllocateInfo::default()
                        .allocation_size(req.size)
                        .memory_type_index(kind),
                    None,
                )?,
            );
            self.device.bind_image_memory(image, memory, 0)?;
            made.keep();
            Ok(Image { image, memory })
        }
    }

    /// Records `f` into a command buffer, runs it and waits.
    pub fn submit_once(&self, f: impl FnOnce(vk::CommandBuffer)) -> Result<()> {
        // SAFETY: a private pool and buffer, freed before returning.
        unsafe {
            let pool = self.device.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(self.family),
                None,
            )?;
            let run = || -> Result<()> {
                let cmd = *self
                    .device
                    .allocate_command_buffers(
                        &vk::CommandBufferAllocateInfo::default()
                            .command_pool(pool)
                            .command_buffer_count(1),
                    )?
                    .first()
                    .ok_or_else(|| anyhow!("no command buffer"))?;
                self.device.begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                f(cmd);
                self.device.end_command_buffer(cmd)?;
                self.device.queue_submit(
                    self.queue,
                    &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                    vk::Fence::null(),
                )?;
                self.device.queue_wait_idle(self.queue)?;
                Ok(())
            };
            let r = run();
            self.device.destroy_command_pool(pool, None);
            r
        }
    }

    /// `layers` square RGBA8 textures of `size` (`texels`, layer after
    /// layer) as one sampled array, uploaded and ready for the shaders.
    pub fn texture_array(
        &self,
        size: u32,
        texels: &[u8],
        layers: u32,
    ) -> Result<(Image, vk::ImageView)> {
        self.texture((size, size), vk::Format::R8G8B8A8_UNORM, texels, layers)
    }

    /// `layers` textures of `size` in `format` (`texels`, layer after
    /// layer, as tightly packed as the format) as one sampled array.
    pub fn texture(
        &self,
        size: (u32, u32),
        format: vk::Format,
        texels: &[u8],
        layers: u32,
    ) -> Result<(Image, vk::ImageView)> {
        let extent = vk::Extent3D {
            width: size.0,
            height: size.1,
            depth: 1,
        };
        // SAFETY: the image is ours until returned; the staging buffer is
        // `scratch`'s, freed after the upload has been waited for (or failed).
        let (mut made, mut scratch) = unsafe { (Cleanup::new(self), Cleanup::new(self)) };
        let image = made.image(
            self.image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(extent)
                    .mip_levels(1)
                    .array_layers(layers)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED),
            )?,
        );
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: layers,
        };
        let staging = scratch.buffer(self.buffer_with(texels, vk::BufferUsageFlags::TRANSFER_SRC)?);
        self.submit_once(|cmd| {
            let barrier = |from, to, src, dst| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(from)
                    .new_layout(to)
                    .src_access_mask(src)
                    .dst_access_mask(dst)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image.image)
                    .subresource_range(range)
            };
            // SAFETY: `cmd` is recording; the image and staging buffer live
            // until the submit has finished.
            unsafe {
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier(
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::AccessFlags::empty(),
                        vk::AccessFlags::TRANSFER_WRITE,
                    )],
                );
                self.device.cmd_copy_buffer_to_image(
                    cmd,
                    staging.buffer,
                    image.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::BufferImageCopy {
                        image_subresource: vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: 0,
                            base_array_layer: 0,
                            layer_count: layers,
                        },
                        image_extent: extent,
                        ..Default::default()
                    }],
                );
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier(
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::SHADER_READ,
                    )],
                );
            }
        })?;
        // SAFETY: a view of the image just made.
        let view = unsafe {
            self.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image.image)
                    .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
                    .format(format)
                    .subresource_range(range),
                None,
            )?
        };
        made.keep();
        drop(scratch);
        Ok((image, view))
    }

    /// Linear filtering, clamped or repeating (`c2-pc gpu.rs: sampler(true,
    /// repeat)`).
    pub fn linear_sampler(&self, repeat: bool) -> Result<vk::Sampler> {
        let a = if repeat {
            vk::SamplerAddressMode::REPEAT
        } else {
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        };
        // SAFETY: plain object creation on our device.
        Ok(unsafe {
            self.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(a)
                    .address_mode_v(a)
                    .address_mode_w(a),
                None,
            )?
        })
    }

    pub fn shader(&self, words: &[u32]) -> Result<vk::ShaderModule> {
        // SAFETY: SPIR-V from our own shaders.
        Ok(unsafe {
            self.device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(words), None)?
        })
    }

    /// # Safety
    /// `b` must belong to this device and must not have been destroyed. All GPU
    /// uses must be complete. Never use `b` again after this call.
    pub unsafe fn destroy_buffer(&self, b: &Buffer) {
        // SAFETY: called once, after the GPU is idle.
        unsafe {
            self.device.unmap_memory(b.memory);
            self.device.destroy_buffer(b.buffer, None);
            self.device.free_memory(b.memory, None);
        }
    }

    /// # Safety
    /// `i` must belong to this device and must not have been destroyed. Destroy
    /// dependent views and complete all GPU uses first; never use `i` again.
    pub unsafe fn destroy_image(&self, i: &Image) {
        // SAFETY: called once, after the GPU is idle.
        unsafe {
            self.device.destroy_image(i.image, None);
            self.device.free_memory(i.memory, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_writes_check_bounds_before_pointer_arithmetic() {
        let mut storage = [0u8; 8];
        let buffer = Buffer {
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            ptr: storage.as_mut_ptr(),
            size: storage.len() as u64,
        };
        buffer.write(8, b"ignored");
        buffer.write(u64::MAX, b"ignored");
        buffer.write(0, b"");
        assert_eq!(storage, [0; 8]);
        buffer.write(6, b"abcd");
        assert_eq!(storage, [0, 0, 0, 0, 0, 0, b'a', b'b']);
        buffer.write(0, b"12345678");
        assert_eq!(&storage, b"12345678");
    }

    /// A plain Vulkan device (no OpenXR), freed after `f` whatever it returns.
    fn with_device(f: impl FnOnce(&Gpu) -> Result<()>) -> Result<()> {
        // SAFETY: an instance and device made and destroyed here, the device
        // idle and its objects freed by `f` before it goes.
        unsafe {
            let entry = ash::Entry::load()?;
            let app = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
            let instance = entry.create_instance(
                &vk::InstanceCreateInfo::default().application_info(&app),
                None,
            )?;
            let made = (|| -> Result<_> {
                let physical = *instance
                    .enumerate_physical_devices()?
                    .first()
                    .ok_or_else(|| anyhow!("no GPU"))?;
                let family = instance
                    .get_physical_device_queue_family_properties(physical)
                    .iter()
                    .position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                    .ok_or_else(|| anyhow!("no graphics queue"))?
                    as u32;
                let device = instance.create_device(
                    physical,
                    &vk::DeviceCreateInfo::default().queue_create_infos(&[
                        vk::DeviceQueueCreateInfo::default()
                            .queue_family_index(family)
                            .queue_priorities(&[1.0]),
                    ]),
                    None,
                )?;
                Ok((physical, family, device))
            })();
            let (physical, family, device) = match made {
                Ok(m) => m,
                Err(e) => {
                    instance.destroy_instance(None);
                    return Err(e);
                }
            };
            let gpu = Gpu::new(instance, device, physical, family);
            let r = f(&gpu);
            let _ = gpu.device.device_wait_idle();
            gpu.device.destroy_device(None);
            gpu.instance.destroy_instance(None);
            r
        }
    }

    /// Run with `--ignored` on a machine with a Vulkan GPU (with
    /// `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`, where installed, a
    /// leaked object is reported when the device goes).
    #[test]
    #[ignore = "needs a Vulkan GPU"]
    fn constructors_and_cleanup_on_a_device() -> Result<()> {
        use crate::scene::Scene;
        let format = vk::Format::R8G8B8A8_SRGB;
        with_device(|gpu| {
            crate::test_scene::TestScene::new(gpu, format)?.destroy(gpu);
            let render_pass = crate::scene::render_pass(gpu, format, None)?;
            crate::panel::Panel::new(gpu, render_pass)?.destroy(gpu);
            // A guard dropped without `keep` frees everything it was given.
            {
                // SAFETY: ours and unused.
                let mut made = unsafe { Cleanup::new(gpu) };
                made.add(render_pass);
                made.buffer(gpu.buffer(64, vk::BufferUsageFlags::VERTEX_BUFFER)?);
                made.texture(gpu.texture_array(4, &[0; 64], 1)?);
            }
            Ok(())
        })
    }
}
