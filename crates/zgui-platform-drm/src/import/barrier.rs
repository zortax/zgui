//! Handing a drawn image over to the display engine.
//!
//! wgpu owns an imported image from the moment `create_texture_from_hal` returns, and it
//! transitions the image for whatever it records. It never releases one to
//! `VK_QUEUE_FAMILY_FOREIGN_EXT`, and only that release makes the pixels the frame drew the pixels
//! the display engine reads. So this crate submits that one barrier itself.
//!
//! # Ordering against the frame
//!
//! The barrier goes on the queue wgpu submitted the frame on. `vkCmdPipelineBarrier`'s first
//! synchronisation scope covers every command submitted to that queue before it that its source
//! stages name, so a barrier-only command buffer submitted after the frame is ordered after the
//! frame. No semaphore, no second queue, and nothing for the caller to hold.
//!
//! # Recording
//!
//! Everything, once. The barrier for one image never changes, so one command buffer per image is
//! recorded while the set is made and submitted again per frame. A frame costs one
//! `vkQueueSubmit` and one wait.
//!
//! # The wait
//!
//! [`Release::submit`] blocks until the barrier has run. The plane is handed no sync file, so
//! nothing tells the display engine to wait for the frame, and the barrier has to have finished
//! before the flip is committed.
//!
//! The wait carries a deadline. A submission a driver never completes is a real failure on this
//! path, and a wait with no deadline turns it into a program that stops and says nothing.
//!
//! # wgpu's own tracker
//!
//! wgpu still holds the image in `COLOR_ATTACHMENT_OPTIMAL` and in this queue family, because
//! nothing tells it otherwise. Both are untrue after a release, and that is safe here because a
//! frame is drawn into the whole of the image: the contents before a frame are read by nothing. A
//! renderer that loaded a buffer's old contents would need the other half of the pair, an acquire
//! barrier ahead of the frame.

use std::fmt;
use std::time::Duration;

use ash::vk;
use tracing::warn;
use zgui_render_wgpu::Gpu;

use crate::import::{Imported, Unsupported, vulkan};

/// How long one barrier is given to finish before the device is called stuck.
///
/// A barrier over one image touches no memory, so two seconds is far past anything a working
/// device takes. The deadline exists for the submission that never completes at all.
const FINISHED: Duration = Duration::from_secs(2);

/// The part of a scanout image a barrier covers: the colour aspect, one level, one layer.
///
/// The image was created with exactly that shape, and a barrier naming any other part of it would
/// leave the rest owned by this queue family and unreadable by the display.
const WHOLE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

/// The barrier that gives a set of drawn images to the display engine.
///
/// One of these per buffer set, holding one recorded command buffer per buffer.
///
/// # What has to outlive it
///
/// The [`Gpu`] it was recorded on. The pool, the command buffers and the fence are Vulkan objects
/// this owns, reached through a device handle that keeps nothing alive, so a device dropped first
/// leaves this destroying objects on a device that has gone. A [`Scanout`](crate::Scanout) holds
/// this beside the textures of the same set, which keep the device open for as long as they exist,
/// and gives this back first.
pub struct Release {
    /// The device the pool, the command buffers and the fence belong to.
    device: ash::Device,
    /// The queue the frame was submitted on, and therefore the one the barrier goes on.
    queue: vk::Queue,
    /// The pool the command buffers were allocated from.
    pool: vk::CommandPool,
    /// One recorded barrier per buffer, at that buffer's own index.
    commands: Vec<vk::CommandBuffer>,
    /// What a submission is waited on.
    fence: vk::Fence,
    /// Whether a submission was left running.
    ///
    /// A wait that ran out leaves the fence pending, and a fence that is pending may be neither
    /// reset nor handed to another submission nor destroyed. So the whole object refuses
    /// everything from then on, which is the honest answer for a device that is not finishing
    /// work.
    stuck: bool,
}

impl Release {
    /// One recorded barrier per buffer of `buffers`, on the queue `gpu` draws with.
    ///
    /// `buffers` has to be the set the renderer will draw into, created on `gpu`. A slot handed to
    /// [`Release::submit`] is a place in this list.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported`] for the reasons [`Imported::create`] returns it, and
    /// [`Unsupported::Driver`] when the driver refuses the pool, the command buffers, the
    /// recording or the fence. Whatever was made before a refusal is given back.
    pub fn record(gpu: &Gpu, buffers: &[Imported]) -> Result<Self, Unsupported> {
        let count = u32::try_from(buffers.len()).ok().filter(|count| *count > 0);
        let Some(count) = count else {
            return Err(Unsupported::Driver {
                step: "recording the barriers that release a frame to the display",
                reason: format!("{} buffers is not a set to record for", buffers.len()),
            });
        };

        vulkan(gpu, |_, device| {
            let raw = device.raw_device();
            let family = device.queue_family_index();

            let info = vk::CommandPoolCreateInfo::default().queue_family_index(family);
            // SAFETY: the family index is the one this device's own queue was created from, and
            // the structure is Vulkan's own with its `sType` set by `default()`.
            let pool = unsafe { raw.create_command_pool(&info, None) }.map_err(|error| {
                refused("creating the pool the barriers are recorded in", error)
            })?;
            // From here on the pool exists, so every later refusal has to give it back.
            let mut building = Recording {
                device: raw,
                pool,
                fence: vk::Fence::null(),
            };

            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(count);
            // SAFETY: the pool was created on this device immediately above and nothing else is
            // allocating from it. The command buffers it hands out live until the pool is
            // destroyed, and `Recording` and `Drop` destroy it.
            let commands = unsafe { raw.allocate_command_buffers(&info) }
                .map_err(|error| refused("allocating the command buffers", error))?;

            for (command, buffer) in commands.iter().zip(buffers) {
                record(raw, *command, buffer.image(), family)?;
            }

            // SAFETY: the structure is Vulkan's own with its `sType` set, and it asks for an
            // unsignalled fence, which the first submission below is handed and signals.
            building.fence = unsafe { raw.create_fence(&vk::FenceCreateInfo::default(), None) }
                .map_err(|error| refused("creating the fence a barrier is waited on", error))?;

            let (pool, fence) = building.take();
            Ok(Self {
                // A handle and a table of function pointers. It keeps nothing alive, and the type
                // documentation states that the caller answers for that.
                device: raw.clone(),
                queue: device.raw_queue(),
                pool,
                commands,
                fence,
                stuck: false,
            })
        })
    }

    /// Gives the buffer at `slot` to the display engine, and waits until it is there.
    ///
    /// Submitted after the renderer's own frame and on the same queue, which is what orders it
    /// after that frame. The caller commits the flip once this returns.
    ///
    /// **The image has to have been drawn into through a render pass since it was made.** The
    /// barrier states `COLOR_ATTACHMENT_OPTIMAL` as the layout it starts from, because that is
    /// what wgpu leaves an image it drew into; an image nothing has drawn into is still
    /// `UNDEFINED` and the barrier describes it wrongly.
    ///
    /// Nothing else may submit on this queue while this runs. Vulkan asks for that of every queue,
    /// and here it is kept by both submissions being made from the frame loop's own thread.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] when `slot` names no buffer, when the driver refuses the
    /// submission, and when the barrier does not finish inside its deadline. The last one is a
    /// device that is not completing work: it is reported once and every later call is refused,
    /// because a fence that is still pending may not be reused.
    pub fn submit(&mut self, slot: usize) -> Result<(), Unsupported> {
        if self.stuck {
            return Err(Unsupported::Driver {
                step: "releasing a frame to the display",
                reason: "an earlier barrier was left running on this device".to_owned(),
            });
        }
        let Some(&command) = self.commands.get(slot) else {
            return Err(Unsupported::Driver {
                step: "releasing a frame to the display",
                reason: format!(
                    "slot {slot} names none of the {} buffers this set holds",
                    self.commands.len()
                ),
            });
        };

        let commands = [command];
        let submit = vk::SubmitInfo::default().command_buffers(&commands);
        // SAFETY: the command buffer was recorded once and every earlier submission of it was
        // waited on, so it is not in use. The fence is unsignalled and not pending, for the same
        // reason. Nothing else submits on this queue while this runs, which the caller answers
        // for.
        unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit), self.fence)
        }
        .map_err(|error| refused("submitting the barrier that releases the frame", error))?;

        let fences = [self.fence];
        let deadline = u64::try_from(FINISHED.as_nanos()).unwrap_or(u64::MAX);
        // SAFETY: the fence was created on this device and was just handed to the submission
        // above, so it is a fence of this device that a submission will signal.
        match unsafe { self.device.wait_for_fences(&fences, true, deadline) } {
            Ok(()) => {}
            Err(vk::Result::TIMEOUT) => {
                self.stuck = true;
                return Err(Unsupported::Driver {
                    step: "waiting for the barrier that releases the frame",
                    reason: format!(
                        "the device did not finish a barrier over one image inside {FINISHED:?}, \
                         so it is not completing the work it is given"
                    ),
                });
            }
            Err(error) => {
                self.stuck = true;
                return Err(refused(
                    "waiting for the barrier that releases the frame",
                    error,
                ));
            }
        }

        // SAFETY: the wait above returned, so the fence is signalled and no submission still names
        // it.
        unsafe { self.device.reset_fences(&fences) }
            .map_err(|error| refused("readying the fence for the next frame", error))
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        if self.stuck {
            // Destroying a pool a submission is still running from, or a fence still pending, is
            // undefined. The device is not finishing work, so there is nothing to wait for either:
            // the objects are left where they are and the driver reclaims them when the process
            // ends.
            warn!(
                "a barrier was left running on the graphics device, so the pool and the fence it \
                 used stay allocated until this process ends"
            );
            return;
        }
        // SAFETY: every submission this made was waited to completion before it returned, so
        // nothing names the pool, its command buffers or the fence. Destroying the pool frees the
        // command buffers with it. The device outlives this, which the type documentation states
        // the caller answers for.
        unsafe {
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}

/// The queue, the pool and how many barriers were recorded.
///
/// Written by hand because `ash::Device` states no [`fmt::Debug`], and a [`Scanout`](crate::Scanout)
/// holding one of these is derived.
impl fmt::Debug for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Release")
            .field("queue", &self.queue)
            .field("pool", &self.pool)
            .field("barriers", &self.commands.len())
            .field("stuck", &self.stuck)
            .finish()
    }
}

/// The pool and the fence while the rest of the recording is still being made.
///
/// Gives both back when it is dropped, so a driver that refuses a step half way through leaves the
/// device as it was found. [`Recording::take`] is what a finished recording stops it with.
struct Recording<'a> {
    /// The device both belong to.
    device: &'a ash::Device,
    /// The pool, which owns every command buffer allocated from it.
    pool: vk::CommandPool,
    /// The fence, or null before it exists.
    fence: vk::Fence,
}

impl Recording<'_> {
    /// The pool and the fence, with this guard disarmed.
    fn take(mut self) -> (vk::CommandPool, vk::Fence) {
        let pool = std::mem::replace(&mut self.pool, vk::CommandPool::null());
        let fence = std::mem::replace(&mut self.fence, vk::Fence::null());
        (pool, fence)
    }
}

impl Drop for Recording<'_> {
    fn drop(&mut self) {
        if self.fence != vk::Fence::null() {
            // SAFETY: the fence was created on this device and was never handed to a submission —
            // it is created last, and a recording that reached a submission is one this guard was
            // already disarmed for.
            unsafe { self.device.destroy_fence(self.fence, None) };
        }
        if self.pool != vk::CommandPool::null() {
            // SAFETY: the pool was created on this device, nothing was ever submitted from it, and
            // destroying it frees the command buffers allocated from it.
            unsafe { self.device.destroy_command_pool(self.pool, None) };
        }
    }
}

/// Records the one barrier that gives `image` to the display engine.
///
/// The layout goes to `GENERAL`, which is the one every layout modifier can be scanned out of, and
/// the ownership goes to `VK_QUEUE_FAMILY_FOREIGN_EXT`, which is what the kernel and the display
/// engine are on the other side of.
///
/// The source stage is every stage rather than the colour attachment output alone. What a frame is
/// recorded as belongs to wgpu, and a source scope narrower than what it recorded would order this
/// after part of the frame. The source access is the colour attachment write and nothing else,
/// because the image is created for that one usage and can be written no other way.
///
/// The destination scope is empty. A release states none: the acquire on the other side is what
/// carries it, and here the other side is a display engine that does not speak Vulkan.
fn record(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    family: u32,
) -> Result<(), Unsupported> {
    let begin = vk::CommandBufferBeginInfo::default();
    // SAFETY: the command buffer was allocated from a pool nothing else records into, and it is
    // recorded once before anything submits it.
    unsafe { device.begin_command_buffer(command, &begin) }
        .map_err(|error| refused("beginning the barrier's command buffer", error))?;

    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(vk::AccessFlags::empty())
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(family)
        .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
        .image(image)
        .subresource_range(WHOLE);
    // SAFETY: the command buffer is recording, the image was created on this device with the
    // subresource shape `WHOLE` states, and the barrier is Vulkan's own structure with its `sType`
    // set by `default()`. `VK_EXT_queue_family_foreign` is among the extensions the device was
    // opened with, so the foreign family index is legal here.
    unsafe {
        device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            std::slice::from_ref(&barrier),
        );
    }

    // SAFETY: the command buffer was begun above and holds one barrier.
    unsafe { device.end_command_buffer(command) }
        .map_err(|error| refused("ending the barrier's command buffer", error))
}

/// Returns a refusal naming the step the driver would not do, and what it answered.
fn refused(step: &'static str, error: vk::Result) -> Unsupported {
    Unsupported::Driver {
        step,
        reason: format!("the driver answered {error:?}"),
    }
}

#[cfg(test)]
mod tests {
    //! The one thing here a device cannot help with: what the barrier is made of.
    //!
    //! The barrier itself needs a driver, and `tests/imported.rs` is where it is submitted. What
    //! is checked here is the description — a subresource range that covered less than the image
    //! would leave part of it owned by this queue family, and the display would scan out whatever
    //! that part held before.

    use super::WHOLE;
    use ash::vk;

    #[test]
    fn a_barrier_covers_the_whole_of_a_scanout_image() {
        assert_eq!(WHOLE.aspect_mask, vk::ImageAspectFlags::COLOR);
        assert_eq!(WHOLE.base_mip_level, 0);
        assert_eq!(WHOLE.level_count, 1, "a scanout image has one level");
        assert_eq!(WHOLE.base_array_layer, 0);
        assert_eq!(WHOLE.layer_count, 1, "and one layer");
    }

    #[test]
    fn the_display_engine_is_on_the_foreign_queue_family() {
        // `VK_QUEUE_FAMILY_FOREIGN_EXT` is `!2`, and it is the one an image goes to for anything
        // outside Vulkan, down to a device that speaks none of it. `VK_QUEUE_FAMILY_EXTERNAL` is
        // `!1` and names queues on the same physical device and driver version as this instance.
        // The two are one digit apart and neither is refused.
        assert_eq!(vk::QUEUE_FAMILY_FOREIGN_EXT, !2);
        assert_ne!(vk::QUEUE_FAMILY_FOREIGN_EXT, vk::QUEUE_FAMILY_EXTERNAL);
    }
}
