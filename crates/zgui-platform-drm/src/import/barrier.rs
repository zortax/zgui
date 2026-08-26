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
//! submitted before it is ordered before it. No second queue, and nothing for the caller to order
//! for itself.
//!
//! # Recording
//!
//! Everything, once. Neither barrier for one image ever changes, so two command buffers per image
//! are recorded while the set is made and submitted again per frame. A frame costs two
//! `vkQueueSubmit` calls and, where the display can be handed a sync file, no wait at all.
//!
//! Every one of them is recorded for **simultaneous use**, because a frame no longer waits for the
//! barriers of the frame three before it. Without that flag, resubmitting a command buffer whose
//! last submission may still be running is undefined.
//!
//! # Who waits for the graphics device
//!
//! The kernel, wherever it can. [`Handover::release`] signals a semaphore exported as a sync file
//! and answers the descriptor, and the caller hands that to the atomic commit as the plane's
//! `IN_FENCE_FD`. The commit then returns at once and the display engine reads the buffer when the
//! frame is finished, so the thread that draws and reads input blocks on nothing.
//!
//! This program, where it cannot. A driver that exports no sync file and a display on the legacy
//! interface, which commits no plane property, both leave one place for the wait to happen: here,
//! before the commit. [`Handover::waits`] counts how often that happened.
//!
//! [`Handover::acquire`] waits in neither case. Queue order alone puts its barrier ahead of the
//! frame, which is all the frame needs: `vkCmdPipelineBarrier` names every command submitted to
//! the queue before it. The wait that used to be here said that nothing this type had submitted
//! was still running, and that is now said where it is needed. [`Drop`] submits one more barrier,
//! which orders after everything, and waits for that.
//!
//! Every wait carries a deadline. A submission a driver never completes is a real failure on this
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
use std::os::fd::OwnedFd;
use std::time::Duration;

use ash::vk;
use tracing::{info, warn};
use zgui_render_wgpu::Gpu;

use crate::import::fence::Signal;
use crate::import::{Imported, Unsupported, vulkan};

/// How long one barrier is given to finish before the device is called stuck.
///
/// A barrier over one image touches no memory, so two seconds is far past anything a working
/// device takes. The deadline exists for the submission that never completes at all.
const FINISHED: Duration = Duration::from_secs(2);

/// The flag every command buffer here is recorded with.
///
/// A frame no longer waits for anything the frame three before it submitted, so a command buffer
/// can be handed to the queue again while its last submission is still running. Vulkan allows that
/// for a command buffer recorded with this flag, and a command buffer recorded without it must not
/// be submitted while it is pending.
///
/// It costs a driver the right to assume one submission at a time of a buffer holding one barrier
/// over one image.
const REUSED: vk::CommandBufferUsageFlags = vk::CommandBufferUsageFlags::SIMULTANEOUS_USE;

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
/// One of these per buffer set, holding the two recorded barriers of each buffer, which side of the
/// handover it is on, and the semaphore a released frame signals.
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
/// The [`Gpu`] this was recorded on has to outlive it. The pool, the command buffers, the fence
/// and the semaphore are Vulkan objects this owns, reached through a device handle that keeps
/// nothing alive, so a device dropped first leaves this destroying objects on a device that has
/// gone. A [`Scanout`](crate::Scanout) holds this beside the textures of the same set, which keep
/// the device open for as long as they exist, and gives this back first.
pub struct Handover {
    /// The device the pool, the command buffers, the fence and the semaphore belong to.
    device: ash::Device,
    /// The queue the frames are submitted on, and therefore the one the barriers go on.
    queue: vk::Queue,
    /// The pool the command buffers were allocated from.
    pool: vk::CommandPool,
    /// The two barriers of each buffer, at that buffer's own index.
    sides: Vec<Sides>,
    /// A barrier over nothing, which orders after every command submitted before it.
    ///
    /// What a wait is put behind when there is no other command to attach one to. Submitting it
    /// and waiting for its fence says that every earlier submission on this queue has finished.
    /// [`Drop`] needs that, and a frame does not.
    drain: vk::CommandBuffer,
    /// What a submission is waited on.
    fence: vk::Fence,
    /// The semaphore a released frame signals, where the kernel can be handed a sync file for it.
    ///
    /// `None` on a display that can take no fence and on a driver that exports none. Either one
    /// leaves the wait in [`Handover::release`].
    signal: Option<Signal>,
    /// How many times this blocked the calling thread on the graphics device.
    waits: usize,
    /// Whether a submission was left running.
    ///
    /// A wait that ran out leaves the fence pending, and a fence that is pending may be neither
    /// reset nor handed to another submission nor destroyed. So the whole object refuses
    /// everything from then on, which is the honest answer for a device that is not finishing
    /// work.
    stuck: bool,
}

impl Handover {
    /// Records the two barriers of every buffer of `buffers`, on the queue `gpu` draws with.
    ///
    /// `buffers` has to be the set the renderer will draw into, created on `gpu`. A slot handed to
    /// [`Handover::acquire`] or [`Handover::release`] is a place in this list.
    ///
    /// `takes_a_fence` says whether the display this set is for can be committed with a sync file,
    /// which [`waits_for_a_fence`](zgui_drm::commit::waits_for_a_fence) answers. It is half the
    /// decision; the other half is whether this driver exports one at all, which is asked here. A
    /// display that gets a yes to both waits for nothing on this thread. A no to either keeps the
    /// wait, and which of the two it was is written to the log once.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported`] for the reasons [`Imported::create`] returns it, and
    /// [`Unsupported::Driver`] when the driver refuses the pool, the command buffers, the
    /// recording, the fence or the semaphore. Whatever was made before a refusal is given back.
    pub fn record(
        gpu: &Gpu,
        buffers: &[Imported],
        takes_a_fence: bool,
    ) -> Result<Self, Unsupported> {
        // Two command buffers per image plus the one a teardown waits behind, and a set of no
        // buffers is not a set to record for.
        let count = u32::try_from(buffers.len())
            .ok()
            .filter(|count| *count > 0)
            .and_then(|count| count.checked_mul(2))
            .and_then(|count| count.checked_add(1));
        let Some(count) = count else {
            return Err(Unsupported::Driver {
                step: "recording the barriers that pass a frame to the display",
                reason: format!("{} buffers is not a set to record for", buffers.len()),
            });
        };

        vulkan(gpu, |adapter, device| {
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
                signal: None,
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

            // Two per buffer in buffer order, and the drain at the end.
            let (&drain, pairs) = commands.split_last().ok_or_else(|| Unsupported::Driver {
                step: "recording the barriers that pass a frame to the display",
                reason: "the driver allocated no command buffer at all".to_owned(),
            })?;
            let mut sides = Vec::with_capacity(buffers.len());
            for (pair, buffer) in pairs.chunks_exact(2).zip(buffers) {
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
            empty(raw, drain)?;

            // SAFETY: the structure is Vulkan's own with its `sType` set, and it asks for an
            // unsignalled fence, which the first submission below is handed and signals.
            building.fence = unsafe { raw.create_fence(&vk::FenceCreateInfo::default(), None) }
                .map_err(|error| refused("creating the fence a barrier is waited on", error))?;

            // Both halves of the one decision, in the order their answers cost anything: a display
            // that can carry no fence is asked nothing of the driver.
            building.signal = if takes_a_fence {
                Signal::create(
                    adapter.shared_instance().raw_instance(),
                    adapter.raw_physical_device(),
                    raw,
                    adapter.shared_instance().instance_api_version(),
                )?
            } else {
                None
            };
            info!(
                fenced = building.signal.is_some(),
                display = takes_a_fence,
                "the graphics device is waited for by {}",
                if building.signal.is_some() {
                    "the kernel, which is handed a sync file per frame"
                } else if takes_a_fence {
                    "this program, because the graphics driver exports no sync file"
                } else {
                    "this program, because this display can be committed with no fence"
                }
            );

            let (pool, fence, signal) = building.take();
            Ok(Self {
                // A handle and a table of function pointers. It keeps nothing alive, and the type
                // documentation states that the caller answers for that.
                device: raw.clone(),
                queue: device.raw_queue(),
                pool,
                sides,
                drain,
                fence,
                signal,
                waits: 0,
                stuck: false,
            })
        })
    }

    /// Takes the buffer at `slot` back from the display engine.
    ///
    /// Submitted before the frame that draws into that buffer is recorded, which makes wgpu's own
    /// idea of the image true again: the buffer is back in `COLOR_ATTACHMENT_OPTIMAL`
    /// and back in this queue family before wgpu records a barrier out of what it believes. A
    /// frame is then free to load the buffer's old contents.
    ///
    /// **Nothing waits here.** The queue itself orders the barrier ahead of the frame, which is all
    /// the frame needs. See the head of this module for what the wait that used to be here said,
    /// and where that is said now.
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
        self.submit(command, &[], vk::Fence::null(), step)?;
        self.sides[slot].foreign = false;
        Ok(())
    }

    /// Gives the buffer at `slot` to the display engine, and says what to wait for it with.
    ///
    /// Submitted after the renderer's own frame and on the same queue, which orders it after that
    /// frame. The caller commits the flip once this returns.
    ///
    /// **The descriptor answered is a sync file the commit is handed as the plane's
    /// `IN_FENCE_FD`**, and it is why this returns without waiting: the kernel holds the flip back
    /// until the frame and this barrier have run. The caller closes it, on a commit that succeeded
    /// and on one that was refused alike — [`Commit::flip`](zgui_drm::commit::Commit::flip) states
    /// why.
    ///
    /// `None` says there is nothing for the kernel to wait on, because the frame is already
    /// finished: this blocked until the barrier had run before it returned. Every display gets that
    /// answer on a driver that exports no sync file, and so does one that can be committed with no
    /// fence at all.
    ///
    /// **The buffer has to be in `COLOR_ATTACHMENT_OPTIMAL`**, which is the layout this states it
    /// starts from. Two things put it there and one of them has to have happened: a render pass
    /// wgpu recorded into it, or the [`Handover::acquire`] that took it back from the display
    /// engine. A buffer of a new set that nothing has drawn into is still `UNDEFINED`, and this
    /// barrier would describe it wrongly.
    ///
    /// Nothing else may submit on this queue while this runs. Vulkan asks for that of every queue,
    /// and here it is kept by both submissions being made from the frame loop's own thread.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] when `slot` names no buffer, when the display engine
    /// already holds that buffer, when the driver refuses the submission, and when a barrier does
    /// not finish inside its deadline. The last one is a device that is not completing work: it is
    /// reported once and every later call is refused, because a fence that is still pending may not
    /// be reused.
    pub fn release(&mut self, slot: usize) -> Result<Option<OwnedFd>, Unsupported> {
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

        let Some(semaphore) = self.signal.as_ref().and_then(Signal::semaphore) else {
            let fence = self.fence;
            self.submit(command, &[], fence, step)?;
            self.wait(step)?;
            self.sides[slot].foreign = true;
            return Ok(None);
        };

        self.submit(command, &[semaphore], vk::Fence::null(), step)?;
        // The barrier is on its way whatever the export does next, so the buffer has changed sides
        // before anything else can fail.
        self.sides[slot].foreign = true;

        let exported = match &self.signal {
            Some(signal) => signal.export(),
            // The semaphore above came out of this very field and nothing between the two takes it
            // away, so this arm is unreachable. It is written out instead of unwrapped, because a
            // frame loop is the wrong place to find out that a `None` was possible after all.
            None => Ok(None),
        };
        match exported {
            Ok(fence) => Ok(fence),
            Err(refusal) => {
                // The semaphore now holds a signal nothing will consume, so nothing may signal it
                // again. What is left is the wait this was meant to remove, and the frame still
                // has to be finished before the caller commits it.
                warn!(
                    "the graphics driver would not export a sync file, so this program waits for \
                     every frame from here on: {refusal}"
                );
                if let Some(signal) = self.signal.as_mut() {
                    signal.refused();
                }
                self.empty_the_queue(step)?;
                Ok(None)
            }
        }
    }

    /// Returns how many times this blocked the calling thread on the graphics device.
    ///
    /// Zero for the whole life of a display the kernel waits for, because a sync file per frame
    /// takes the wait off this thread. One per frame drawn on every other display, plus one for the
    /// teardown that empties the queue.
    ///
    /// Published because it is the one part of the arrangement a caller can check instead of
    /// believing it: a frame that still waits looks exactly like a frame that does not, until this
    /// climbs.
    pub fn waits(&self) -> usize {
        self.waits
    }

    /// Submits one recorded command buffer, or says why the driver would not take it.
    ///
    /// `signal` is signalled when everything in the submission has run, and `fence` says nothing
    /// when it is null. Nothing here waits.
    fn submit(
        &mut self,
        command: vk::CommandBuffer,
        signal: &[vk::Semaphore],
        fence: vk::Fence,
        step: &'static str,
    ) -> Result<(), Unsupported> {
        if self.stuck {
            return Err(Unsupported::Driver {
                step,
                reason: "an earlier barrier was left running on this device".to_owned(),
            });
        }

        let commands = [command];
        let submit = vk::SubmitInfo::default()
            .command_buffers(&commands)
            .signal_semaphores(signal);
        // SAFETY: the command buffer was recorded for simultaneous use, so submitting it while an
        // earlier submission of it is still running is allowed. `fence` is either null or this
        // object's own, which is unsignalled and named by no other submission — every path that
        // hands it over waits for it before it returns. The semaphore is unsignalled as well: the
        // export that follows every signal of it moves the payload out, and a refused export stops
        // it being signalled at all. Nothing else submits on this queue while this runs, which the
        // caller answers for.
        unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit), fence)
        }
        .map_err(|error| refused(step, error))
    }

    /// Waits for the submission that was handed the fence, and readies the fence for the next one.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] when the submission does not finish inside [`FINISHED`] and
    /// when the driver refuses the wait. Both leave the whole object refusing everything
    /// afterwards, because a fence that is still pending may be neither reset nor reused nor
    /// destroyed.
    fn wait(&mut self, step: &'static str) -> Result<(), Unsupported> {
        self.waits += 1;
        let fences = [self.fence];
        let deadline = u64::try_from(FINISHED.as_nanos()).unwrap_or(u64::MAX);
        // SAFETY: the fence was created on this device and was just handed to a submission, so it
        // is a fence of this device that a submission will signal.
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

    /// Waits until every submission this made has finished.
    ///
    /// The drain barrier names every command submitted to this queue before it, so it starts only
    /// once all of them are done and the fence it carries says so. That is what a wait attaches to
    /// when there is no other command to attach one to. A fence handed to a submission of its own
    /// says nothing about the submissions before it.
    ///
    /// # Errors
    ///
    /// The ones [`Handover::wait`] returns.
    fn empty_the_queue(&mut self, step: &'static str) -> Result<(), Unsupported> {
        let (drain, fence) = (self.drain, self.fence);
        self.submit(drain, &[], fence, step)?;
        self.wait(step)
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
        // The one wait a frame no longer does, done here instead. A pool may not be destroyed
        // while a command buffer allocated from it is still running, and a frame leaves its
        // barriers running on purpose.
        if !self.stuck {
            drop(self.empty_the_queue("emptying the queue before the barriers are given back"));
        }
        if self.stuck {
            // Destroying a pool a submission is still running from, a fence still pending or a
            // semaphore a submission still names is undefined. The device is not finishing work,
            // so there is nothing to wait for either: the objects are left where they are and the
            // driver reclaims them when the process ends.
            warn!(
                "a barrier was left running on the graphics device, so the pool, the fence and \
                 the semaphore it used stay allocated until this process ends"
            );
            return;
        }
        // SAFETY: the drain above finished, and it is ordered after every command submitted to
        // this queue before it, so nothing names the pool, its command buffers, the fence or the
        // semaphore. Destroying the pool frees the command buffers with it. The device outlives
        // this, which the type documentation states the caller answers for.
        unsafe {
            if let Some(signal) = &self.signal {
                signal.destroy(&self.device);
            }
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
            .field("fenced", &self.signal.is_some())
            .field("waits", &self.waits)
            .field("stuck", &self.stuck)
            .finish()
    }
}

/// The pool, the fence and the semaphore while the rest of the recording is still being made.
///
/// Gives all three back when it is dropped, so a driver that refuses a step half way through leaves
/// the device as it was found. [`Recording::take`] is what a finished recording stops it with.
struct Recording<'a> {
    /// The device all three belong to.
    device: &'a ash::Device,
    /// The pool, which owns every command buffer allocated from it.
    pool: vk::CommandPool,
    /// The fence, or null before it exists.
    fence: vk::Fence,
    /// The semaphore a released frame signals, before it exists and where there is none.
    signal: Option<Signal>,
}

impl Recording<'_> {
    /// Returns the pool, the fence and the semaphore, and disarms this guard.
    fn take(mut self) -> (vk::CommandPool, vk::Fence, Option<Signal>) {
        let pool = std::mem::replace(&mut self.pool, vk::CommandPool::null());
        let fence = std::mem::replace(&mut self.fence, vk::Fence::null());
        let signal = self.signal.take();
        (pool, fence, signal)
    }
}

impl Drop for Recording<'_> {
    fn drop(&mut self) {
        if let Some(signal) = &self.signal {
            // SAFETY: the semaphore was created on this device and was never handed to a
            // submission — it is created last, and a recording that reached a submission is one
            // this guard was already disarmed for.
            unsafe { signal.destroy(self.device) };
        }
        if self.fence != vk::Fence::null() {
            // SAFETY: as above, for the same reason.
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
    let begin = vk::CommandBufferBeginInfo::default().flags(REUSED);
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

/// Records a barrier over nothing into `command`, which orders after every earlier command.
///
/// A pipeline barrier naming no image and no buffer is an execution dependency and nothing else,
/// and `ALL_COMMANDS` on its first side makes that dependency cover every command already submitted
/// to the queue. So a fence handed to the submission that carries this signals only once all of
/// them have finished. A fence handed to a submission of its own says no such thing.
fn empty(device: &ash::Device, command: vk::CommandBuffer) -> Result<(), Unsupported> {
    let begin = vk::CommandBufferBeginInfo::default().flags(REUSED);
    // SAFETY: the command buffer was allocated from a pool nothing else records into, and it is
    // recorded once before anything submits it.
    unsafe { device.begin_command_buffer(command, &begin) }
        .map_err(|error| refused("beginning the drain barrier's command buffer", error))?;

    // SAFETY: the command buffer is recording, and a barrier with no memory barrier of any kind is
    // an execution dependency the specification defines for every queue that takes graphics work.
    unsafe {
        device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[],
        );
    }

    // SAFETY: the command buffer was begun above and holds one barrier.
    unsafe { device.end_command_buffer(command) }
        .map_err(|error| refused("ending the drain barrier's command buffer", error))
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
