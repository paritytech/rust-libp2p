//! The state of all active streams in a QUIC connection

use super::stream::StreamState;
use futures::channel::oneshot;
use std::collections::HashMap;
use std::task;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
/// A stream ID.
pub(super) struct StreamId(quinn_proto::StreamId);

impl std::ops::Deref for StreamId {
    type Target = quinn_proto::StreamId;
    fn deref(&self) -> &quinn_proto::StreamId {
        &self.0
    }
}

/// A set of streams.
#[derive(Debug, Default)]
pub(super) struct Streams {
    map: HashMap<quinn_proto::StreamId, StreamState>,
}

impl Streams {
    pub(super) fn add_stream(&mut self, id: quinn_proto::StreamId) -> StreamId {
        if self.map.insert(id, Default::default()).is_some() {
            panic!(
                "Internal state corrupted. \
                You probably used a Substream with the wrong StreamMuxer",
            )
        }
        StreamId(id)
    }

    fn get(&mut self, id: &StreamId) -> &mut StreamState {
        self.map.get_mut(id).expect(
            "Internal state corrupted. \
            You probably used a Substream with the wrong StreamMuxer",
        )
    }

    /// Indicate that the stream is open for reading. Calling this when nobody
    /// is waiting for this stream to be readable is a harmless no-op.
    pub(super) fn wake_reader(&mut self, id: quinn_proto::StreamId) {
        if let Some(stream) = self.map.get_mut(&id) {
            stream.wake_reader()
        }
    }

    /// If a task is waiting for this stream to be finished or written to, wake
    /// it up. Otherwise, do nothing.
    pub(super) fn wake_writer(&mut self, id: quinn_proto::StreamId) {
        if let Some(stream) = self.map.get_mut(&id) {
            stream.wake_writer()
        }
    }

    /// Set a waker that will be notified when the state becomes readable.
    /// Wake up any waker that has already been registered.
    pub(super) fn set_reader(&mut self, id: &StreamId, waker: task::Waker) {
        self.get(id).set_reader(waker);
    }

    /// Set a waker that will be notified when the task becomes writable or is
    /// finished, waking up any waker or channel that has already been
    /// registered.
    pub(super) fn set_writer(&mut self, id: &StreamId, waker: task::Waker) {
        self.get(id).set_writer(waker);
    }

    /// Set a channel that will be notified when the task becomes writable or is
    /// finished, waking up any existing registered waker or channel
    pub(super) fn set_finisher(&mut self, id: &StreamId, finisher: oneshot::Sender<()>) {
        self.get(id).set_finisher(finisher);
    }

    /// Remove an ID from the map
    pub(super) fn remove(&mut self, id: StreamId) {
        if self.map.remove(&id.0).is_none() {
            panic!(
                "Internal state corrupted. \
                You probably used a Substream with the wrong StreamMuxer",
            );
        }
    }

    pub(super) fn wake_all(&mut self) {
        for i in self.map.values_mut() {
            i.wake_all()
        }
    }

    pub(super) fn keys(
        &self,
    ) -> std::collections::hash_map::Keys<'_, quinn_proto::StreamId, StreamState> {
        self.map.keys()
    }
}