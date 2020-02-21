//! The per-stream state in a QUIC connection
use futures::channel::oneshot;
use std::{mem, task};

/// The state of a writer.
#[derive(Debug)]
enum WriterStatus {
    /// Nobody is waiting to write to or finish this stream.
    Unblocked,
    /// Writing to the stream is blocked. `waker` is the waker used to wake up
    /// the writer task.
    Blocked { waker: task::Waker },
    /// The stream is being shut down. `finisher` is the channel used to notify
    /// the finishing task.
    Finishing { finisher: oneshot::Sender<()> },
}

impl WriterStatus {
    fn take(self) {
        match self {
            Self::Unblocked => {}
            Self::Blocked { waker } => waker.wake(),
            Self::Finishing { finisher } => {
                let _ = finisher.send(());
            }
        }
    }
}

/// The state of a stream. Dropping this will wake up any tasks blocked on it.
#[derive(Debug)]
pub(crate) struct StreamState {
    /// If a task is blocked on reading this stream, this holds a waker that
    /// will wake it up. Otherwise, it is `None`.
    reader: Option<task::Waker>,
    /// The state of the writing portion of this stream.
    writer: WriterStatus,
}

impl Default for StreamState {
    fn default() -> Self {
        Self {
            reader: None,
            writer: WriterStatus::Unblocked,
        }
    }
}

impl Drop for StreamState {
    fn drop(&mut self) {
        self.wake_all()
    }
}

impl StreamState {
    /// Indicate that the stream is open for reading. Calling this when nobody
    /// is waiting for this stream to be readable is a harmless no-op.
    pub(crate) fn wake_reader(&mut self) {
        if let Some(waker) = self.reader.take() {
            waker.wake()
        }
    }

    /// If a task is waiting for this stream to be finished or written to, wake
    /// it up. Otherwise, do nothing.
    pub(crate) fn wake_writer(&mut self) {
        mem::replace(&mut self.writer, WriterStatus::Unblocked).take()
    }

    /// Set a waker that will be notified when the state becomes readable.
    /// Wake up any waker that has already been registered.
    pub(crate) fn set_reader(&mut self, waker: task::Waker) {
        if let Some(waker) = mem::replace(&mut self.reader, Some(waker)) {
            waker.wake()
        }
    }

    /// Set a waker that will be notified when the task becomes writable or is
    /// finished, waking up any waker or channel that has already been
    /// registered.
    pub(crate) fn set_writer(&mut self, waker: task::Waker) {
        mem::replace(&mut self.writer, WriterStatus::Blocked { waker }).take()
    }

    /// Set a channel that will be notified when the task becomes writable or is
    /// finished, waking up any existing registered waker or channel
    pub(crate) fn set_finisher(&mut self, finisher: oneshot::Sender<()>) {
        mem::replace(&mut self.writer, WriterStatus::Finishing { finisher }).take()
    }

    /// Wake up both readers and writers. This is just a shorthand for calling
    /// [`Self::wake_writer`] and [`Self::wake_reader`].
    pub(crate) fn wake_all(&mut self) {
        self.wake_writer();
        self.wake_reader();
    }
}