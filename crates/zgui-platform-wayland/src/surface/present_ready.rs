//! A dedicated callback queue lets presentation readiness progress while the UI is busy.
use rustix::event::{PollFd, PollFlags, poll};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::Arc,
    task::{Context, Wake, Waker},
    time::Duration,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    protocol::wl_callback::{self, WlCallback},
};

#[derive(Default)]
pub(crate) struct ReadyState {
    seen: Option<u64>,
}
impl Dispatch<WlCallback, u64> for ReadyState {
    fn event(
        state: &mut Self,
        _proxy: &WlCallback,
        _event: wl_callback::Event,
        serial: &u64,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        state.seen = Some(state.seen.map_or(*serial, |old| old.max(*serial)));
    }
}
struct QueueWake(UnixStream);
impl Wake for QueueWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = (&self.0).write(&[1]);
    }
}

pub(crate) struct ReadyQueue {
    queue: EventQueue<ReadyState>,
    state: ReadyState,
    notified: UnixStream,
    waker: Waker,
}
impl ReadyQueue {
    pub(super) fn new(connection: &Connection) -> Self {
        let (notified, notify) = UnixStream::pair().expect("presentation callback wake pair");
        notified
            .set_nonblocking(true)
            .expect("nonblocking callback wake");
        notify
            .set_nonblocking(true)
            .expect("nonblocking callback wake");
        Self {
            queue: connection.new_event_queue(),
            state: ReadyState::default(),
            notified,
            waker: Arc::new(QueueWake(notify)).into(),
        }
    }
    pub(super) fn handle(&self) -> QueueHandle<ReadyState> {
        self.queue.handle()
    }
    pub(super) fn dispatch(&mut self) -> Option<u64> {
        let mut bytes = [0; 64];
        while (&self.notified).read(&mut bytes).is_ok_and(|n| n > 0) {}
        let mut context = Context::from_waker(&self.waker);
        let _ = self
            .queue
            .poll_dispatch_pending(&mut context, &mut self.state);
        self.state.seen.take()
    }
    pub(super) fn wait(&mut self) -> Option<u64> {
        if let Some(serial) = self.dispatch() {
            return Some(serial);
        }
        let _ = self.queue.flush();
        let Some(guard) = self.queue.prepare_read() else {
            return self.dispatch();
        };
        let mut descriptors = [
            PollFd::new(&self.queue, PollFlags::IN),
            PollFd::new(&self.notified, PollFlags::IN),
        ];
        let timeout = Duration::from_millis(50).try_into().unwrap();
        if poll(&mut descriptors, Some(&timeout)).is_ok()
            && descriptors[0].revents().contains(PollFlags::IN)
        {
            let _ = guard.read();
        } else {
            drop(guard);
        }
        self.dispatch()
    }
}
