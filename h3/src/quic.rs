//! QUIC Transport traits
//!
//! This module includes traits and types meant to allow being generic over any
//! QUIC implementation.

use core::fmt;
use std::task::{self, Poll};

use bytes::{Buf, Bytes};

pub use crate::proto::stream::{InvalidStreamId, StreamId};
pub use crate::stream::WriteBuf;

// Unresolved questions:
//
// - Should the `poll_` methods be `Pin<&mut Self>`?

/// Trait that represent an error from the transport layer
pub trait Error: std::error::Error + Send + Sync {
    /// Check if the current error is a transport timeout
    fn is_timeout(&self) -> bool;

    /// Get the QUIC error code from connection close or stream stop
    fn err_code(&self) -> Option<u64>;
}

impl<'a, E: Error + 'a> From<E> for Box<dyn Error + 'a> {
    fn from(err: E) -> Box<dyn Error + 'a> {
        Box::new(err)
    }
}

/// Types of erros that *specifically* arises when sending a datagram.
#[derive(Debug)]
pub enum SendDatagramError {
    /// Datagrams are not supported by the peer
    UnsupportedByPeer,
    /// Datagrams are locally disabled
    Disabled,
    /// The datagram was too large to be sent.
    TooLarge,
    /// Network error
    ConnectionLost(Box<dyn Error>),
}

impl fmt::Display for SendDatagramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendDatagramError::UnsupportedByPeer => write!(f, "datagrams not supported by peer"),
            SendDatagramError::Disabled => write!(f, "datagram support disabled"),
            SendDatagramError::TooLarge => write!(f, "datagram too large"),
            SendDatagramError::ConnectionLost(_) => write!(f, "connection lost"),
        }
    }
}

impl std::error::Error for SendDatagramError {}

impl Error for SendDatagramError {
    fn is_timeout(&self) -> bool {
        false
    }

    fn err_code(&self) -> Option<u64> {
        match self {
            Self::ConnectionLost(err) => err.err_code(),
            _ => None,
        }
    }
}

/// Trait representing a QUIC connection.
pub trait Connection {
    /// The type produced by `poll_accept_bidi()`
    type BidiStream: SendStream + RecvStream;
    /// The type of the sending part of `BidiStream`
    type SendStream: SendStream;
    /// The type produced by `poll_accept_recv()`
    type RecvStream: RecvStream;
    /// A producer of outgoing Unidirectional and Bidirectional streams.
    type OpenStreams: OpenStreams<
        SendStream = Self::SendStream,
        RecvStream = Self::RecvStream,
        BidiStream = Self::BidiStream,
    >;
    /// Error type yielded by this trait methods
    type Error: Into<Box<dyn Error>>;

    /// Accept an incoming unidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_recv(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::RecvStream>, Self::Error>>;

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_bidi(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::BidiStream>, Self::Error>>;

    /// Poll the connection to create a new bidirectional stream.
    fn poll_open_bidi(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::BidiStream, Self::Error>>;

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_send(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>>;

    /// Get an object to open outgoing streams.
    fn opener(&self) -> Self::OpenStreams;

    /// Close the connection immediately
    fn close(&mut self, code: crate::error::Code, reason: &[u8]);

    /// Poll the connection for incoming datagrams.
    fn poll_accept_datagram(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Bytes>, Self::Error>>;

    /// Send a datagram
    fn send_datagram(&mut self, data: Bytes) -> Result<(), SendDatagramError>;
}

/// Trait for opening outgoing streams
pub trait OpenStreams {
    /// The type produced by `poll_open_bidi()`
    type BidiStream: SendStream + RecvStream;
    /// The type produced by `poll_open_send()`
    type SendStream: SendStream;
    /// The type of the receiving part of `BidiStream`
    type RecvStream: RecvStream;
    /// Error type yielded by these trait methods
    type Error: Into<Box<dyn Error>>;

    /// Poll the connection to create a new bidirectional stream.
    fn poll_open_bidi(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::BidiStream, Self::Error>>;

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>>;

    /// Close the connection immediately
    fn close(&mut self, code: crate::error::Code, reason: &[u8]);
}

/// A trait describing the "send" actions of a QUIC stream.
pub trait SendStream {
    /// The error type returned by fallible send methods.
    type Error: Into<Box<dyn Error>>;

    /// Attempts write data into the stream.
    ///
    /// Returns the number of bytes written.
    ///
    /// `buf` is advanced by the number of bytes written.
    ///
    /// This allows writing arbitrary data to the stream as well as complete encoded frames.
    fn poll_send<D: Buf>(
        &mut self,
        cx: &mut task::Context<'_>,
        buf: &mut D,
    ) -> Poll<Result<usize, Self::Error>>;

    /// Poll to finish the sending side of the stream.
    fn poll_finish(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>>;

    /// Send a QUIC reset code.
    fn reset(&mut self, reset_code: u64);

    /// Get QUIC send stream id
    fn send_id(&self) -> StreamId;
}

/// A trait describing the "receive" actions of a QUIC stream.
pub trait RecvStream {
    /// The type of `Buf` for data received on this stream.
    type Buf: Buf;
    /// The error type that can occur when receiving data.
    type Error: 'static + Error;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this should return `None`.
    fn poll_data(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::Buf>, Self::Error>>;

    /// Send a `STOP_SENDING` QUIC code.
    fn stop_sending(&mut self, error_code: u64);

    /// Get QUIC send stream id
    fn recv_id(&self) -> StreamId;
}

/// Optional trait to allow "splitting" a bidirectional stream into two sides.
pub trait BidiStream: SendStream + RecvStream {
    /// The type for the send half.
    type SendStream: SendStream;
    /// The type for the receive half.
    type RecvStream: RecvStream;

    /// Split this stream into two halves.
    fn split(self) -> (Self::SendStream, Self::RecvStream);
}
