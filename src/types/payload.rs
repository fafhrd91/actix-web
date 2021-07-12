//! Basic binary and string payload extractors.

use std::{
    borrow::Cow,
    future::Future,
    pin::Pin,
    str,
    task::{Context, Poll},
};

use actix_http::error::PayloadError;
use actix_utils::future::{ready, Either, Ready};
use bytes::{Bytes, BytesMut};
use encoding_rs::{Encoding, UTF_8};
use futures_core::{ready, stream::Stream};
use mime::Mime;

use crate::{
    dev, error::ErrorBadRequest, http::header, web, Error, FromRequest, HttpMessage,
    HttpRequest,
};

/// Extract a request's raw payload stream.
///
/// See [`PayloadConfig`] for important notes when using this advanced extractor.
///
/// # Examples
/// ```
/// use std::future::Future;
/// use futures_util::stream::StreamExt as _;
/// use actix_web::{post, web};
///
/// // `body: web::Payload` parameter extracts raw payload stream from request
/// #[post("/")]
/// async fn index(mut body: web::Payload) -> actix_web::Result<String> {
///     // for demonstration only; in a normal case use the `Bytes` extractor
///     // collect payload stream into a bytes object
///     let mut bytes = web::BytesMut::new();
///     while let Some(item) = body.next().await {
///         bytes.extend_from_slice(&item?);
///     }
///
///     Ok(format!("Request Body Bytes:\n{:?}", bytes))
/// }
/// ```
pub struct Payload(pub crate::dev::Payload);

impl Payload {
    /// Unwrap to inner Payload type.
    pub fn into_inner(self) -> crate::dev::Payload {
        self.0
    }
}

impl Stream for Payload {
    type Item = Result<Bytes, PayloadError>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.0).poll_next(cx)
    }
}

/// See [here](#usage) for example of usage as an extractor.
impl FromRequest for Payload {
    type Error = Error;
    type Future = Ready<Result<Payload, Error>>;

    #[inline]
    fn from_request(_: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        ready(Ok(Payload(payload.take())))
    }
}

/// Extract binary data from a request's payload.
///
/// Collects request payload stream into a [Bytes] instance.
///
/// Use [`PayloadConfig`] to configure extraction process.
///
/// # Examples
/// ```
/// use actix_web::{post, web};
///
/// /// extract binary data from request
/// #[post("/")]
/// async fn index(body: web::Bytes) -> String {
///     format!("Body {:?}!", body)
/// }
/// ```
impl FromRequest for Bytes {
    type Error = Error;
    type Future = Either<BytesExtractFut, Ready<Result<Bytes, Error>>>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        // allow both Config and Data<Config>
        let cfg = PayloadConfig::from_req(req);

        if let Err(err) = cfg.check_mimetype(req) {
            return Either::right(ready(Err(err)));
        }

        Either::left(BytesExtractFut {
            body_fut: HttpMessageBody::new(req, payload).limit(cfg.limit),
        })
    }
}

/// Future for `Bytes` extractor.
pub struct BytesExtractFut {
    body_fut: HttpMessageBody,
}

impl<'a> Future for BytesExtractFut {
    type Output = Result<Bytes, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.body_fut).poll(cx).map_err(Into::into)
    }
}

/// Extract text information from a request's body.
///
/// Text extractor automatically decode body according to the request's charset.
///
/// Use [`PayloadConfig`] to configure extraction process.
///
/// # Examples
/// ```
/// use actix_web::{post, web, FromRequest};
///
/// // extract text data from request
/// #[post("/")]
/// async fn index(text: String) -> String {
///     format!("Body {}!", text)
/// }
impl FromRequest for String {
    type Error = Error;
    type Future = Either<StringExtractFut, Ready<Result<String, Error>>>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        let cfg = PayloadConfig::from_req(req);

        // check content-type
        if let Err(err) = cfg.check_mimetype(req) {
            return Either::right(ready(Err(err)));
        }

        // check charset
        let encoding = match req.encoding() {
            Ok(enc) => enc,
            Err(err) => return Either::right(ready(Err(err.into()))),
        };
        let limit = cfg.limit;
        let body_fut = HttpMessageBody::new(req, payload).limit(limit);

        Either::left(StringExtractFut { body_fut, encoding })
    }
}

/// Future for `String` extractor.
pub struct StringExtractFut {
    body_fut: HttpMessageBody,
    encoding: &'static Encoding,
}

impl<'a> Future for StringExtractFut {
    type Output = Result<String, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let encoding = self.encoding;

        Pin::new(&mut self.body_fut).poll(cx).map(|out| {
            let body = out?;
            bytes_to_string(body, encoding)
        })
    }
}

fn bytes_to_string(body: Bytes, encoding: &'static Encoding) -> Result<String, Error> {
    if encoding == UTF_8 {
        Ok(str::from_utf8(body.as_ref())
            .map_err(|_| ErrorBadRequest("Can not decode body"))?
            .to_owned())
    } else {
        Ok(encoding
            .decode_without_bom_handling_and_without_replacement(&body)
            .map(Cow::into_owned)
            .ok_or_else(|| ErrorBadRequest("Can not decode body"))?)
    }
}

/// Configuration for request payloads.
///
/// Applies to the built-in [`Bytes`] and [`String`] extractors.
/// Note that the [`Payload`] extractor does not automatically check
/// conformance with this configuration to allow more flexibility when
/// building extractors on top of [`Payload`].
///
/// By default, the payload size limit is 256kB and there is no mime type condition.
///
/// To use this, add an instance of it to your [`app`](crate::App), [`scope`](crate::Scope)
/// or [`resource`](crate::Resource) through the associated `.app_data()` method.
#[derive(Clone)]
pub struct PayloadConfig {
    limit: usize,
    mimetype: Option<Mime>,
}

impl PayloadConfig {
    /// Create new instance with a size limit (in bytes) and no mime type condition.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    /// Set maximum accepted payload size in bytes. The default limit is 256kB.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set required mime type of the request. By default mime type is not enforced.
    pub fn mimetype(mut self, mt: Mime) -> Self {
        self.mimetype = Some(mt);
        self
    }

    fn check_mimetype(&self, req: &HttpRequest) -> Result<(), Error> {
        // check content-type
        if let Some(ref mt) = self.mimetype {
            match req.mime_type() {
                Ok(Some(ref req_mt)) => {
                    if mt != req_mt {
                        return Err(ErrorBadRequest("Unexpected Content-Type"));
                    }
                }
                Ok(None) => {
                    return Err(ErrorBadRequest("Content-Type is expected"));
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }
        Ok(())
    }

    /// Extract payload config from app data. Check both `T` and `Data<T>`, in that order, and fall
    /// back to the default payload config if neither is found.
    fn from_req(req: &HttpRequest) -> &Self {
        req.app_data::<Self>()
            .or_else(|| req.app_data::<web::Data<Self>>().map(|d| d.as_ref()))
            .unwrap_or(&DEFAULT_CONFIG)
    }
}

/// Allow shared refs used as defaults.
const DEFAULT_CONFIG: PayloadConfig = PayloadConfig {
    limit: DEFAULT_CONFIG_LIMIT,
    mimetype: None,
};

const DEFAULT_CONFIG_LIMIT: usize = 262_144; // 2^18 bytes (~256kB)

impl Default for PayloadConfig {
    fn default() -> Self {
        DEFAULT_CONFIG.clone()
    }
}

/// Future that resolves to a complete HTTP body payload.
///
/// By default only 256kB payload is accepted before `PayloadError::Overflow` is returned.
/// Use `MessageBody::limit()` method to change upper limit.
pub struct HttpMessageBody {
    limit: usize,
    length: Option<usize>,
    #[cfg(feature = "__compress")]
    stream: dev::Decompress<dev::Payload>,
    #[cfg(not(feature = "__compress"))]
    stream: dev::Payload,
    buf: BytesMut,
    err: Option<PayloadError>,
}

impl HttpMessageBody {
    /// Create `MessageBody` for request.
    #[allow(clippy::borrow_interior_mutable_const)]
    pub fn new(req: &HttpRequest, payload: &mut dev::Payload) -> HttpMessageBody {
        let mut length = None;
        let mut err = None;

        if let Some(l) = req.headers().get(&header::CONTENT_LENGTH) {
            match l.to_str() {
                Ok(s) => match s.parse::<usize>() {
                    Ok(l) => {
                        if l > DEFAULT_CONFIG_LIMIT {
                            err = Some(PayloadError::Overflow);
                        }
                        length = Some(l)
                    }
                    Err(_) => err = Some(PayloadError::UnknownLength),
                },
                Err(_) => err = Some(PayloadError::UnknownLength),
            }
        }

        let stream = {
            cfg_if::cfg_if! {
                if #[cfg(feature = "__compress")] {
                    dev::Decompress::from_headers(payload.take(), req.headers())
                } else {
                    payload.take()
                }
            }
        };

        HttpMessageBody {
            stream,
            limit: DEFAULT_CONFIG_LIMIT,
            length,
            buf: BytesMut::with_capacity(8192),
            err,
        }
    }

    /// Change max size of payload. By default max size is 256kB
    pub fn limit(mut self, limit: usize) -> Self {
        if let Some(l) = self.length {
            self.err = if l > limit {
                Some(PayloadError::Overflow)
            } else {
                None
            };
        }
        self.limit = limit;
        self
    }
}

impl Future for HttpMessageBody {
    type Output = Result<Bytes, PayloadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        if let Some(err) = this.err.take() {
            return Poll::Ready(Err(err));
        }

        loop {
            let res = ready!(Pin::new(&mut this.stream).poll_next(cx));
            match res {
                Some(chunk) => {
                    let chunk = chunk?;
                    if this.buf.len() + chunk.len() > this.limit {
                        return Poll::Ready(Err(PayloadError::Overflow));
                    } else {
                        this.buf.extend_from_slice(&chunk);
                    }
                }
                None => return Poll::Ready(Ok(this.buf.split().freeze())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::http::{header, StatusCode};
    use crate::test::{call_service, init_service, TestRequest};
    use crate::{web, App, Responder};

    #[actix_rt::test]
    async fn test_payload_config() {
        let req = TestRequest::default().to_http_request();
        let cfg = PayloadConfig::default().mimetype(mime::APPLICATION_JSON);
        assert!(cfg.check_mimetype(&req).is_err());

        let req = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .to_http_request();
        assert!(cfg.check_mimetype(&req).is_err());

        let req = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .to_http_request();
        assert!(cfg.check_mimetype(&req).is_ok());
    }

    // allow deprecated App::data
    #[allow(deprecated)]
    #[actix_rt::test]
    async fn test_config_recall_locations() {
        async fn bytes_handler(_: Bytes) -> impl Responder {
            "payload is probably json bytes"
        }

        async fn string_handler(_: String) -> impl Responder {
            "payload is probably json string"
        }

        let srv = init_service(
            App::new()
                .service(
                    web::resource("/bytes-app-data")
                        .app_data(PayloadConfig::default().mimetype(mime::APPLICATION_JSON))
                        .route(web::get().to(bytes_handler)),
                )
                .service(
                    web::resource("/bytes-data")
                        .data(PayloadConfig::default().mimetype(mime::APPLICATION_JSON))
                        .route(web::get().to(bytes_handler)),
                )
                .service(
                    web::resource("/string-app-data")
                        .app_data(PayloadConfig::default().mimetype(mime::APPLICATION_JSON))
                        .route(web::get().to(string_handler)),
                )
                .service(
                    web::resource("/string-data")
                        .data(PayloadConfig::default().mimetype(mime::APPLICATION_JSON))
                        .route(web::get().to(string_handler)),
                ),
        )
        .await;

        let req = TestRequest::with_uri("/bytes-app-data").to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let req = TestRequest::with_uri("/bytes-data").to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let req = TestRequest::with_uri("/string-app-data").to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let req = TestRequest::with_uri("/string-data").to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let req = TestRequest::with_uri("/bytes-app-data")
            .insert_header(header::ContentType(mime::APPLICATION_JSON))
            .to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let req = TestRequest::with_uri("/bytes-data")
            .insert_header(header::ContentType(mime::APPLICATION_JSON))
            .to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let req = TestRequest::with_uri("/string-app-data")
            .insert_header(header::ContentType(mime::APPLICATION_JSON))
            .to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let req = TestRequest::with_uri("/string-data")
            .insert_header(header::ContentType(mime::APPLICATION_JSON))
            .to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_rt::test]
    async fn test_bytes() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_LENGTH, "11"))
            .set_payload(Bytes::from_static(b"hello=world"))
            .to_http_parts();

        let s = Bytes::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s, Bytes::from_static(b"hello=world"));
    }

    #[actix_rt::test]
    async fn test_string() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_LENGTH, "11"))
            .set_payload(Bytes::from_static(b"hello=world"))
            .to_http_parts();

        let s = String::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s, "hello=world");
    }

    #[actix_rt::test]
    async fn test_message_body() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_LENGTH, "xxxx"))
            .to_srv_request()
            .into_parts();
        let res = HttpMessageBody::new(&req, &mut pl).await;
        match res.err().unwrap() {
            PayloadError::UnknownLength => {}
            _ => unreachable!("error"),
        }

        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_LENGTH, "1000000"))
            .to_srv_request()
            .into_parts();
        let res = HttpMessageBody::new(&req, &mut pl).await;
        match res.err().unwrap() {
            PayloadError::Overflow => {}
            _ => unreachable!("error"),
        }

        let (req, mut pl) = TestRequest::default()
            .set_payload(Bytes::from_static(b"test"))
            .to_http_parts();
        let res = HttpMessageBody::new(&req, &mut pl).await;
        assert_eq!(res.ok().unwrap(), Bytes::from_static(b"test"));

        let (req, mut pl) = TestRequest::default()
            .set_payload(Bytes::from_static(b"11111111111111"))
            .to_http_parts();
        let res = HttpMessageBody::new(&req, &mut pl).limit(5).await;
        match res.err().unwrap() {
            PayloadError::Overflow => {}
            _ => unreachable!("error"),
        }
    }
}
