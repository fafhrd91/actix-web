use std::{
    io::Write,
    pin::Pin,
    task::{Context, Poll},
};

use actix_codec::Framed;
use actix_utils::future::poll_fn;
use bytes::buf::BufMut;
use bytes::{Bytes, BytesMut};
use futures_core::{ready, Stream};
use futures_util::SinkExt as _;

use crate::h1;
use crate::http::{
    header::{HeaderMap, IntoHeaderValue, EXPECT, HOST},
    StatusCode,
};
use crate::message::{RequestHeadType, ResponseHead};
use crate::payload::Payload;
use crate::{error::PayloadError, Error};

use super::connection::{ConnectionIo, H1Connection};
use super::error::{ConnectError, SendRequestError};
use crate::body::{BodySize, MessageBody};

pub(crate) async fn send_request<Io, B>(
    io: H1Connection<Io>,
    mut head: RequestHeadType,
    body: B,
) -> Result<(ResponseHead, Payload), SendRequestError>
where
    Io: ConnectionIo,
    B: MessageBody,
    B::Error: Into<Error>,
{
    // set request host header
    if !head.as_ref().headers.contains_key(HOST)
        && !head.extra_headers().iter().any(|h| h.contains_key(HOST))
    {
        if let Some(host) = head.as_ref().uri.host() {
            let mut wrt = BytesMut::with_capacity(host.len() + 5).writer();

            match head.as_ref().uri.port_u16() {
                None | Some(80) | Some(443) => write!(wrt, "{}", host)?,
                Some(port) => write!(wrt, "{}:{}", host, port)?,
            };

            match wrt.get_mut().split().freeze().try_into_value() {
                Ok(value) => match head {
                    RequestHeadType::Owned(ref mut head) => {
                        head.headers.insert(HOST, value);
                    }
                    RequestHeadType::Rc(_, ref mut extra_headers) => {
                        let headers = extra_headers.get_or_insert(HeaderMap::new());
                        headers.insert(HOST, value);
                    }
                },
                Err(e) => log::error!("Can not set HOST header {}", e),
            }
        }
    }

    // create Framed and prepare sending request
    let mut framed = Framed::new(io, h1::ClientCodec::default());

    // Check EXPECT header and enable expect handle flag accordingly.
    //
    // RFC: https://tools.ietf.org/html/rfc7231#section-5.1.1
    let is_expect = if head.as_ref().headers.contains_key(EXPECT) {
        match body.size() {
            BodySize::None | BodySize::Empty | BodySize::Sized(0) => {
                let keep_alive = framed.codec_ref().keepalive();
                framed.io_mut().on_release(keep_alive);

                // TODO: use a new variant or a new type better describing error violate
                // `Requirements for clients` session of above RFC
                return Err(SendRequestError::Connect(ConnectError::Disconnected));
            }
            _ => true,
        }
    } else {
        false
    };

    let mut pin_framed = Pin::new(&mut framed);

    // special handle for EXPECT request.
    let (do_send, mut res_head) = if is_expect {
        pin_framed.send((head, body.size()).into()).await?;
        let head = poll_fn(|cx| pin_framed.as_mut().poll_next(cx))
            .await
            .ok_or(ConnectError::Disconnected)??;

        // return response head in case status code is not continue
        // and current head would be used as final response head.
        (head.status == StatusCode::CONTINUE, Some(head))
    } else {
        pin_framed.feed((head, body.size()).into()).await?;
        (true, None)
    };

    if do_send {
        // send request body
        match body.size() {
            BodySize::None | BodySize::Empty | BodySize::Sized(0) => {
                poll_fn(|cx| pin_framed.as_mut().flush(cx)).await?;
            }
            _ => send_body(body, pin_framed.as_mut()).await?,
        };

        // read response and init read body
        let head = poll_fn(|cx| pin_framed.as_mut().poll_next(cx))
            .await
            .ok_or(ConnectError::Disconnected)??;

        res_head = Some(head);
    }

    let head = res_head.unwrap();

    match pin_framed.codec_ref().message_type() {
        h1::MessageType::None => {
            let keep_alive = pin_framed.codec_ref().keepalive();
            pin_framed.io_mut().on_release(keep_alive);

            Ok((head, Payload::None))
        }
        _ => Ok((head, Payload::Stream(Box::pin(PlStream::new(framed))))),
    }
}

pub(crate) async fn open_tunnel<Io>(
    io: Io,
    head: RequestHeadType,
) -> Result<(ResponseHead, Framed<Io, h1::ClientCodec>), SendRequestError>
where
    Io: ConnectionIo,
{
    // create Framed and send request.
    let mut framed = Framed::new(io, h1::ClientCodec::default());
    framed.send((head, BodySize::None).into()).await?;

    // read response head.
    let head = poll_fn(|cx| Pin::new(&mut framed).poll_next(cx))
        .await
        .ok_or(ConnectError::Disconnected)??;

    Ok((head, framed))
}

/// send request body to the peer
pub(crate) async fn send_body<Io, B>(
    body: B,
    mut framed: Pin<&mut Framed<Io, h1::ClientCodec>>,
) -> Result<(), SendRequestError>
where
    Io: ConnectionIo,
    B: MessageBody,
    B::Error: Into<Error>,
{
    actix_rt::pin!(body);

    let mut eof = false;
    while !eof {
        while !eof && !framed.as_ref().is_write_buf_full() {
            match poll_fn(|cx| body.as_mut().poll_next(cx)).await {
                Some(Ok(chunk)) => {
                    framed.as_mut().write(h1::Message::Chunk(Some(chunk)))?;
                }
                Some(Err(err)) => return Err(err.into().into()),
                None => {
                    eof = true;
                    framed.as_mut().write(h1::Message::Chunk(None))?;
                }
            }
        }

        if !framed.as_ref().is_write_buf_empty() {
            poll_fn(|cx| match framed.as_mut().flush(cx) {
                Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
                Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
                Poll::Pending => {
                    if !framed.as_ref().is_write_buf_full() {
                        Poll::Ready(Ok(()))
                    } else {
                        Poll::Pending
                    }
                }
            })
            .await?;
        }
    }

    framed.get_mut().flush().await?;
    Ok(())
}

#[pin_project::pin_project]
pub(crate) struct PlStream<Io: ConnectionIo> {
    #[pin]
    framed: Framed<H1Connection<Io>, h1::ClientPayloadCodec>,
}

impl<Io: ConnectionIo> PlStream<Io> {
    fn new(framed: Framed<H1Connection<Io>, h1::ClientCodec>) -> Self {
        let framed = framed.into_map_codec(|codec| codec.into_payload_codec());

        PlStream { framed }
    }
}

impl<Io: ConnectionIo> Stream for PlStream<Io> {
    type Item = Result<Bytes, PayloadError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        match ready!(this.framed.as_mut().next_item(cx)?) {
            Some(Some(chunk)) => Poll::Ready(Some(Ok(chunk))),
            Some(None) => {
                let keep_alive = this.framed.codec_ref().keepalive();
                this.framed.io_mut().on_release(keep_alive);
                Poll::Ready(None)
            }
            None => Poll::Ready(None),
        }
    }
}
