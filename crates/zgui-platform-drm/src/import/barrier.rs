//! Passing an image between the renderer and the display engine.
//!
//! wgpu owns an imported image from the moment `create_texture_from_hal` returns, and it
//! transitions the image for whatever it records. It never releases one to
//! `VK_QUEUE_FAMILY_FOREIGN_EXT`, and only that release makes the pixels the frame drew the pixels
//! the display engine reads. So this crate submits that barrier itself, and the barrier that takes
//! the image back.
//!
//! # The two halves
//!
//! **The release** moves the image from `COLOR_ATTACHMENT_OPTIMAL` to `GENERAL` and gives it from
//! this queue family to `VK_QUEUE_FAMILY_FOREIGN_EXT`. It runs after the frame and before the
//! flip.
//!
//! **The acquire** moves it back from `GENERAL` to `COLOR_ATTACHMENT_OPTIMAL` and takes it from
//! the foreign family. It runs before the frame that draws into that buffer again.
//!
//! Neither half stands alone. Each is one half of an ownership transfer whose other half is
//! outside Vulkan, and a display engine is exactly that. Each states the layout the other one
//! left.
//!
//! # Ordering against the frame
//!
//! Both go on the queue wgpu submits frames on. `vkCmdPipelineBarrier`'s first synchronisation
//! scope covers every command submitted to that queue before it that its source stages name, so a
//! barrier-only command buffer submitted after the frame is ordered after the frame, and one
//! submitted before it is ordered before it. No semaphore, no second queue, and nothing for the
//! caller to hold.
//!
//! # Recording
//!
//! Everything, once. Neither barrier for one image ever changes, so two command buffers per image
//! are recorded while the set is made and submitted again per frame. A frame costs two
//! `vkQueueSubmit` calls and two waits.
//!
//! # The waits
//!
//! [`Handover::release`] blocks until its barrier has run. The plane is handed no sync file, so
//! nothing tells the display engine to wait for the frame, and the barrier has to have finished
//! before the flip is committed.
//!
//! [`Handover::acquire`] blocks for a smaller reason. Queue order alone puts its barrier ahead of
//! the frame, which is all the frame needs. The wait says that every submission this type made has
//! finished by the time the call returns, so [`Drop`] destroys the pool and the fence without
//! asking whether anything is still running.
//!
//! Both waits carry a deadline. A submission a driver never completes is a real failure on this
//! path, and a wait with no deadline turns it into a program that stops and says nothing.
//!
//! # wgpu's own tracker
//!
//! wgpu records a texture handed to it as uninitialised, so the first barrier it lays down over one
//! starts from `UNDEFINED`. After a frame the tracker holds the colour target state, which is
//! `COLOR_ATTACHMENT_OPTIMAL` and this queue family, and nothing here tells it anything else. The
//! pair keeps that true: the release makes it false and the acquire makes it true again, and the
//! acquire runs before wgpu records anything that touches the buffer. So at every point where wgpu
//! reads its own tracker, the tracker is right.
//!
//! A frame is then free to load the buffer's old contents instead of drawing over all of them.
//! Without the acquire it would read pixels the specification calls undefined, on a path where
//! every test still passes.

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

/// Which side of the handover an image is on, and the two barriers that move it.
///
/// Both command buffers are recorded when the set is made and submitted again per frame.
struct Sides {
    /// Takes the image back from the display engine.
    acquire: vk::CommandBuffer,
    /// Gives the image to the display engine.
    release: vk::CommandBuffer,
    /// Whether the display engine holds the image.
    ///
    /// False for a buffer nothing has released yet, which is every buffer of a new set. An acquire
    /// there would state an ownership transfer that never happened and a layout the image has
    /// never been in, so a fresh buffer is acquired by doing nothing.
    foreign: bool,
}

/// Moving a set of images between the renderer and the display engine.
///
/// One of these per buffer set, holding the two recorded barriers of each buffer and which side of
/// the handover it is on.
///
/// # The order
///
/// [`Handover::acquire`] before the frame is recorded, [`Handover::release`] after it is
/// submitted, per buffer. Getting it wrong is refused and never silent: an acquire of a buffer
/// this side of the handover already does nothing, and a release of one the display engine already
/// holds is an error naming the buffer.
///
/// # The device
///
/// The [`Gpu`] this was recorded on has to outlive it. The pool, the command buffers and the fence
/// are Vulkan objects this owns, reached through a device handle that keeps nothing alive, so a
/// device dropped first leaves this destroying objects on a device that has gone. A
/// [`Scanout`](crate::Scanout) holds this beside the textures of the same set, which keep the
/// device open for as long as they exist, and gives this back first.
pub struct Handover {
    /// The device the pool, the command buffers and the fence belong to.
    device: ash::Device,
    /// The queue the frames are submitted on, and therefore the one the barriers go on.
    queue: vk::Queue,
    /// The pool the command buffers were allocated from.
    pool: vk::CommandPool,
    /// The two barriers of each buffer, at that buffer's own index.
    sides: Vec<Sides>,
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

impl Handover {
    /// The two recorded barriers of every buffer of `buffers`, on the queue `gpu` draws with.
    ///
    /// `buffers` has to be the set the renderer will draw into, created on `gpu`. A slot handed to
    /// [`Handover::acquire`] or [`Handover::release`] is a place in this list.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported`] for the reasons [`Imported::create`] returns it, and
    /// [`Unsupported::Driver`] when the driver refuses the pool, the command buffers, the
    /// recording or the fence. Whatever was made before a refusal is given back.
    pub fn record(gpu: &Gpu, buffers: &[Imported]) -> Result<Self, Unsupported> {
        // Two command buffers per image, and a set of none is not a set to record for.
        let count = u32::try_from(buffers.len())
            .ok()
            .and_then(|count| count.checked_mul(2))
            .filter(|count| *count > 0);
        let Some(count) = count else {
            return Err(Unsupported::Driver {
                step: "recording the barriers that pass a frame to the display",
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

            let mut sides = Vec::with_capacity(buffers.len());
            for (pair, buffer) in commands.chunks_exact(2).zip(buffers) {
                let (acquire, release) = (pair[0], pair[1]);
                record(raw, acquire, buffer.image(), Half::Acquire { family })?;
                record(raw, release, buffer.image(), Half::Release { family })?;
                sides.push(Sides {
                    acquire,
                    release,
                    // Nothing has released this buffer, so this side of the handover holds it.
                    foreign: false,
                });
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
                sides,
                fence,
                stuck: false,
            })
        })
    }

    /// Takes the buffer at `slot` back from the display engine, and waits until it is back.
    ///
    /// Submitted before the frame that draws into that buffer is recorded, which makes wgpu's own
    /// idea of the image true again: the buffer is back in `COLOR_ATTACHMENT_OPTIMAL` and back in
    /// this queue family before wgpu records a barrier out of what it believes. A frame is then
    /// free to load the buffer's old contents.
    ///
    /// **Does nothing for a buffer the display engine does not hold**, which covers every buffer
    /// of a new set and every buffer acquired twice in a row. So a caller acquires before every
    /// frame and never works out whether one is owed.
    ///
    /// # Errors
    ///
    /// The ones [`Handover::release`] returns, other than the one about a buffer on the wrong side
    /// of the handover.
    pub fn acquire(&mut self, slot: usize) -> Result<(), Unsupported> {
        let step = "taking a frame back from the display";
        let Some(side) = self.sides.get(slot) else {
            return Err(self.no_such(slot, step));
        };
        if !side.foreign {
            return Ok(());
        }
        let command = side.acquire;
        self.run(command, step)?;
        self.sides[slot].foreign = false;
        Ok(())
    }

    /// Gives the buffer at `slot` to the display engine, and waits until it is there.
    ///
    /// Submitted after the renderer's own frame and on the same queue, which is what orders it
    /// after that frame. The caller commits the flip once this returns.
    ///
    /// **The buffer has to be in `COLOR_ATTACHMENT_OPTIMAL`**, which is the layout this states it
    /// starts from. Two things put it there and one of them has to have happened: a render pass
    /// wgpu recorded into it, or the [`Handover::acquire`] that took it back from the display
    /// engine. A buffer of a new set that nothing has drawn into is still `UNDEFINED`, and this
    /// would describe it wrongly.
    ///
    /// Nothing else may submit on this queue while this runs. Vulkan asks for that of every queue,
    /// and here it is kept by both submissions being made from the frame loop's own thread.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] when `slot` names no buffer, when the display engine
    /// already holds that buffer, when the driver refuses the submission, and when the barrier
    /// does not finish inside its deadline. The last one is a device that is not completing work:
    /// it is reported once and every later call is refused, because a fence that is still pending
    /// may not be reused.
    pub fn release(&mut self, slot: usize) -> Result<(), Unsupported> {
        let step = "giving a frame to the display";
        let Some(side) = self.sides.get(slot) else {
            return Err(self.no_such(slot, step));
        };
        if side.foreign {
            return Err(Unsupported::Driver {
                step,
                reason: format!(
                    "the display engine already holds buffer {slot}, so it has to be acquired \
                     before a frame is drawn into it"
                ),
            });
        }
        let command = side.release;
        self.run(command, step)?;
        self.sides[slot].foreign = true;
        Ok(())
    }

    /// Submits one recorded barrier and waits for it, or says why it could not.
    fn run(&mut self, command: vk::CommandBuffer, step: &'static str) -> Result<(), Unsupported> {
        if self.stuck {
            return Err(Unsupported::Driver {
                step,
                reason: "an earlier barrier was left running on this device".to_owned(),
            });
        }

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
        .map_err(|error| refused(step, error))?;

        let fences = [self.fence];
        let deadline = u64::try_from(FINISHED.as_nanos()).unwrap_or(u64::MAX);
        // SAFETY: the fence was created on this device and was just handed to the submission
        // above, so it is a fence of this device that a submission will signal.
        match unsafe { self.device.wait_for_fences(&fences, true, deadline) } {
            Ok(()) => {}
            Err(vk::Result::TIMEOUT) => {
                self.stuck = true;
                return Err(Unsupported::Driver {
                    step,
                    reason: format!(
                        "the device did not finish a barrier over one image inside {FINISHED:?}, \
                         so it is not completing the work it is given"
                    ),
                });
            }
            Err(error) => {
                self.stuck = true;
                return Err(refused(step, error));
            }
        }

        // SAFETY: the wait above returned, so the fence is signalled and no submission still names
        // it.
        unsafe { self.device.reset_fences(&fences) }
            .map_err(|error| refused("readying the fence for the next barrier", error))
    }

    /// Returns a refusal for a slot that names no buffer of this set.
    fn no_such(&self, slot: usize, step: &'static str) -> Unsupported {
        Unsupported::Driver {
            step,
            reason: format!(
                "slot {slot} names none of the {} buffers this set holds",
                self.sides.len()
            ),
        }
    }
}

impl Drop for Handover {
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

/// The queue, the pool, how many buffers there are and how many the display engine holds.
///
/// Written by hand because `ash::Device` states no [`fmt::Debug`], and a [`Scanout`](crate::Scanout)
/// holding one of these is derived.
impl fmt::Debug for Handover {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Handover")
            .field("queue", &self.queue)
            .field("pool", &self.pool)
            .field("buffers", &self.sides.len())
            .field(
                "on the display",
                &self.sides.iter().filter(|side| side.foreign).count(),
            )
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

/// Which way one barrier moves an image, and the queue family on this side of it.
///
/// Both halves state the same two layouts and the same two families, one the reverse of the other.
/// A transfer whose two halves disagreed would be two transfers, and the image would end up owned
/// by nobody.
#[derive(Debug, Clone, Copy)]
enum Half {
    /// From the display engine back to the renderer.
    Acquire {
        /// The family that takes the image.
        family: u32,
    },
    /// From the renderer to the display engine.
    Release {
        /// The family that gives the image up.
        family: u32,
    },
}

impl Half {
    /// Returns the barrier this half is, over `image`.
    ///
    /// A release ends in `GENERAL` and hands the image to `VK_QUEUE_FAMILY_FOREIGN_EXT`, which is
    /// the family the kernel and the display engine sit behind: the specification defines it for a
    /// reader outside Vulkan, including one that speaks no Vulkan at all. `GENERAL` is the layout
    /// the specification defines every kind of device access in, so it is the one an image is left
    /// in for a reader whose accesses Vulkan cannot name. An acquire is the reverse, ending in the
    /// layout wgpu's tracker holds these images in.
    ///
    /// Each half states a scope on its own side only. The other side of an ownership transfer
    /// carries the other scope, and here that side is a display engine that speaks no Vulkan, so
    /// there is nothing to state for it.
    ///
    /// The renderer's own side is every stage, and not the colour attachment output alone. What a
    /// frame is recorded as belongs to wgpu, and a scope narrower than what it recorded would order
    /// the barrier against part of the frame. The access is the colour attachment and nothing else,
    /// because the image is created for that one usage and can be reached no other way.
    fn barrier(self, image: vk::Image) -> vk::ImageMemoryBarrier<'static> {
        let barrier = vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(WHOLE);
        match self {
            Self::Acquire { family } => barrier
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                )
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .dst_queue_family_index(family),
            Self::Release { family } => barrier
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::empty())
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT),
        }
    }

    /// Returns the stages the barrier waits on, and the stages that wait for it.
    ///
    /// The side that is outside Vulkan states the shortest scope there is: `TOP_OF_PIPE` waits for
    /// nothing and `BOTTOM_OF_PIPE` is waited for by nothing, which is how an ignored scope is
    /// written.
    fn stages(self) -> (vk::PipelineStageFlags, vk::PipelineStageFlags) {
        match self {
            Self::Acquire { .. } => (
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::ALL_COMMANDS,
            ),
            Self::Release { .. } => (
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            ),
        }
    }
}

/// Records the one barrier `half` is, over `image`, into `command`.
fn record(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    half: Half,
) -> Result<(), Unsupported> {
    let begin = vk::CommandBufferBeginInfo::default();
    // SAFETY: the command buffer was allocated from a pool nothing else records into, and it is
    // recorded once before anything submits it.
    unsafe { device.begin_command_buffer(command, &begin) }
        .map_err(|error| refused("beginning a barrier's command buffer", error))?;

    let barrier = half.barrier(image);
    let (source, destination) = half.stages();
    // SAFETY: the command buffer is recording, the image was created on this device with the
    // subresource shape `WHOLE` states, and the barrier is Vulkan's own structure with its `sType`
    // set by `default()`. `VK_EXT_queue_family_foreign` is among the extensions the device was
    // opened with, so the foreign family index is legal here.
    unsafe {
        device.cmd_pipeline_barrier(
            command,
            source,
            destination,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            std::slice::from_ref(&barrier),
        );
    }

    // SAFETY: the command buffer was begun above and holds one barrier.
    unsafe { device.end_command_buffer(command) }
        .map_err(|error| refused("ending a barrier's command buffer", error))
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
    //! The one thing here a device cannot help with: what the two barriers are made of.
    //!
    //! Submitting them needs a driver, and `tests/imported.rs` is where that happens. What is
    //! checked here is the description. A subresource range that covered less than the image would
    //! leave part of it owned by this queue family, and a pair whose halves disagreed would be two
    //! transfers instead of one, leaving the image owned by nobody.

    use super::{Half, WHOLE};
    use ash::vk;

    /// The family the renderer's queue is on, as a number no other constant here is.
    const FAMILY: u32 = 7;

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

    #[test]
    fn a_release_hands_the_image_to_the_display_engine_in_the_layout_it_scans_out_of() {
        let image = vk::Image::null();
        let barrier = Half::Release { family: FAMILY }.barrier(image);

        assert_eq!(
            barrier.old_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(barrier.new_layout, vk::ImageLayout::GENERAL);
        assert_eq!(barrier.src_queue_family_index, FAMILY);
        assert_eq!(barrier.dst_queue_family_index, vk::QUEUE_FAMILY_FOREIGN_EXT);
    }

    #[test]
    fn an_acquire_is_the_release_read_backwards() {
        // The two halves of one ownership transfer state the same layouts and the same families,
        // each the other way round. Halves that disagreed would be two transfers, and the image
        // would come out owned by nobody with every ioctl reporting success.
        let image = vk::Image::null();
        let release = Half::Release { family: FAMILY }.barrier(image);
        let acquire = Half::Acquire { family: FAMILY }.barrier(image);

        assert_eq!(acquire.old_layout, release.new_layout);
        assert_eq!(acquire.new_layout, release.old_layout);
        assert_eq!(
            acquire.src_queue_family_index,
            release.dst_queue_family_index
        );
        assert_eq!(
            acquire.dst_queue_family_index,
            release.src_queue_family_index
        );
    }

    #[test]
    fn each_half_states_a_scope_on_the_renderers_side_only() {
        // The other side of both transfers is a display engine, which speaks no Vulkan and has no
        // scope to state. `TOP_OF_PIPE` waits for nothing and `BOTTOM_OF_PIPE` is waited for by
        // nothing, which is how an ignored scope is written.
        let (source, destination) = Half::Acquire { family: FAMILY }.stages();
        assert_eq!(source, vk::PipelineStageFlags::TOP_OF_PIPE);
        assert_eq!(destination, vk::PipelineStageFlags::ALL_COMMANDS);

        let (source, destination) = Half::Release { family: FAMILY }.stages();
        assert_eq!(source, vk::PipelineStageFlags::ALL_COMMANDS);
        assert_eq!(destination, vk::PipelineStageFlags::BOTTOM_OF_PIPE);

        let image = vk::Image::null();
        assert!(
            Half::Acquire { family: FAMILY }
                .barrier(image)
                .src_access_mask
                .is_empty(),
            "the display engine's own accesses are not Vulkan's to name"
        );
        assert!(
            Half::Release { family: FAMILY }
                .barrier(image)
                .dst_access_mask
                .is_empty(),
            "and neither are the ones it will make next"
        );
    }
}
