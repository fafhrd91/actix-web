//! For URL encoded form helper documentation, see [`Form`].

use std::{
    borrow::Cow,
    fmt,
    future::Future,
    ops,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use actix_http::Payload;
use bytes::BytesMut;
use encoding_rs::{Encoding, UTF_8};
use futures_core::{future::LocalBoxFuture, ready};
use futures_util::{FutureExt as _, StreamExt as _};
use serde::{de::DeserializeOwned, Serialize};

#[cfg(feature = "__compress")]
use crate::dev::Decompress;
use crate::{
    error::UrlencodedError, extract::FromRequest, http::header::CONTENT_LENGTH, web, Error,
    HttpMessage, HttpRequest, HttpResponse, Responder,
};

/// URL encoded payload extractor and responder.
///
/// `Form` has two uses: URL encoded responses, and extracting typed data from URL request payloads.
///
/// # Extractor
/// To extract typed data from a request body, the inner type `T` must implement the
/// [`serde::Deserialize`] trait.
///
/// Use [`FormConfig`] to configure extraction process.
///
/// ```
/// use actix_web::{post, web};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     name: String,
/// }
///
/// // This handler is only called if:
/// // - request headers declare the content type as `application/x-www-form-urlencoded`
/// // - request payload is deserialized into a `Info` struct from the URL encoded format
/// #[post("/")]
/// async fn index(form: web::Form<Info>) -> String {
///     format!("Welcome {}!", form.name)
/// }
/// ```
///
/// # Responder
/// The `Form` type also allows you to create URL encoded responses:
/// simply return a value of type Form<T> where T is the type to be URL encoded.
/// The type  must implement [`serde::Serialize`].
///
/// Responses use
///
/// ```
/// use actix_web::{get, web};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct SomeForm {
///     name: String,
///     age: u8
/// }
///
/// // Response will have:
/// // - status: 200 OK
/// // - header: `Content-Type: application/x-www-form-urlencoded`
/// // - body: `name=actix&age=123`
/// #[get("/")]
/// async fn index() -> web::Form<SomeForm> {
///     web::Form(SomeForm {
///         name: "actix".into(),
///         age: 123
///     })
/// }
/// ```
///
/// # Panics
/// URL encoded forms consist of unordered `key=value` pairs, therefore they cannot be decoded into
/// any type which depends upon data ordering (eg. tuples). Trying to do so will result in a panic.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Form<T>(pub T);

impl<T> Form<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> ops::Deref for Form<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Form<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Serialize for Form<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<T> FromRequest for Form<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = Error;
    type Future = FormExtractFut<T>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let FormConfig { limit, err_handler } = FormConfig::from_req(req).clone();

        FormExtractFut {
            fut: UrlEncoded::new(req, payload).limit(limit),
            req: req.clone(),
            err_handler,
        }
    }
}

type FormErrHandler = Option<Rc<dyn Fn(UrlencodedError, &HttpRequest) -> Error>>;

pub struct FormExtractFut<T> {
    fut: UrlEncoded<T>,
    err_handler: FormErrHandler,
    req: HttpRequest,
}

impl<T> Future for FormExtractFut<T>
where
    T: DeserializeOwned + 'static,
{
    type Output = Result<Form<T>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let res = ready!(Pin::new(&mut this.fut).poll(cx));

        let res = match res {
            Err(err) => match &this.err_handler {
                Some(err_handler) => Err((err_handler)(err, &this.req)),
                None => Err(err.into()),
            },
            Ok(item) => Ok(Form(item)),
        };

        Poll::Ready(res)
    }
}

impl<T: fmt::Display> fmt::Display for Form<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// See [here](#responder) for example of usage as a handler return type.
impl<T: Serialize> Responder for Form<T> {
    fn respond_to(self, _: &HttpRequest) -> HttpResponse {
        match serde_urlencoded::to_string(&self.0) {
            Ok(body) => HttpResponse::Ok()
                .content_type(mime::APPLICATION_WWW_FORM_URLENCODED)
                .body(body),
            Err(err) => HttpResponse::from_error(UrlencodedError::Serialize(err)),
        }
    }
}

/// [`Form`] extractor configuration.
///
/// ```
/// use actix_web::{post, web, App, FromRequest, Result};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     username: String,
/// }
///
/// // Custom `FormConfig` is applied to App.
/// // Max payload size for URL encoded forms is set to 4kB.
/// #[post("/")]
/// async fn index(form: web::Form<Info>) -> Result<String> {
///     Ok(format!("Welcome {}!", form.username))
/// }
///
/// App::new()
///     .app_data(web::FormConfig::default().limit(4096))
///     .service(index);
/// ```
#[derive(Clone)]
pub struct FormConfig {
    limit: usize,
    err_handler: FormErrHandler,
}

impl FormConfig {
    /// Set maximum accepted payload size. By default this limit is 16kB.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set custom error handler
    pub fn error_handler<F>(mut self, f: F) -> Self
    where
        F: Fn(UrlencodedError, &HttpRequest) -> Error + 'static,
    {
        self.err_handler = Some(Rc::new(f));
        self
    }

    /// Extract payload config from app data.
    ///
    /// Checks both `T` and `Data<T>`, in that order, and falls back to the default payload config.
    fn from_req(req: &HttpRequest) -> &Self {
        req.app_data::<Self>()
            .or_else(|| req.app_data::<web::Data<Self>>().map(|d| d.as_ref()))
            .unwrap_or(&DEFAULT_CONFIG)
    }
}

/// Allow shared refs used as default.
const DEFAULT_CONFIG: FormConfig = FormConfig {
    limit: 16_384, // 2^14 bytes (~16kB)
    err_handler: None,
};

impl Default for FormConfig {
    fn default() -> Self {
        DEFAULT_CONFIG
    }
}

/// Future that resolves to some `T` when parsed from a URL encoded payload.
///
/// Form can be deserialized from any type `T` that implements [`serde::Deserialize`].
///
/// Returns error if:
/// - content type is not `application/x-www-form-urlencoded`
/// - content length is greater than [limit](UrlEncoded::limit())
pub struct UrlEncoded<T> {
    #[cfg(feature = "__compress")]
    stream: Option<Decompress<Payload>>,
    #[cfg(not(feature = "__compress"))]
    stream: Option<Payload>,

    limit: usize,
    length: Option<usize>,
    encoding: &'static Encoding,
    err: Option<UrlencodedError>,
    fut: Option<LocalBoxFuture<'static, Result<T, UrlencodedError>>>,
}

#[allow(clippy::borrow_interior_mutable_const)]
impl<T> UrlEncoded<T> {
    /// Create a new future to decode a URL encoded request payload.
    pub fn new(req: &HttpRequest, payload: &mut Payload) -> Self {
        // check content type
        if req.content_type().to_lowercase() != "application/x-www-form-urlencoded" {
            return Self::err(UrlencodedError::ContentType);
        }
        let encoding = match req.encoding() {
            Ok(enc) => enc,
            Err(_) => return Self::err(UrlencodedError::ContentType),
        };

        let mut len = None;
        if let Some(l) = req.headers().get(&CONTENT_LENGTH) {
            if let Ok(s) = l.to_str() {
                if let Ok(l) = s.parse::<usize>() {
                    len = Some(l)
                } else {
                    return Self::err(UrlencodedError::UnknownLength);
                }
            } else {
                return Self::err(UrlencodedError::UnknownLength);
            }
        };

        let payload = {
            cfg_if::cfg_if! {
                if #[cfg(feature = "__compress")] {
                    Decompress::from_headers(payload.take(), req.headers())
                } else {
                    payload.take()
                }
            }
        };

        UrlEncoded {
            encoding,
            stream: Some(payload),
            limit: 32_768,
            length: len,
            fut: None,
            err: None,
        }
    }

    fn err(err: UrlencodedError) -> Self {
        UrlEncoded {
            stream: None,
            limit: 32_768,
            fut: None,
            err: Some(err),
            length: None,
            encoding: UTF_8,
        }
    }

    /// Set maximum accepted payload size. The default limit is 256kB.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

impl<T> Future for UrlEncoded<T>
where
    T: DeserializeOwned + 'static,
{
    type Output = Result<T, UrlencodedError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(ref mut fut) = self.fut {
            return Pin::new(fut).poll(cx);
        }

        if let Some(err) = self.err.take() {
            return Poll::Ready(Err(err));
        }

        // payload size
        let limit = self.limit;
        if let Some(len) = self.length.take() {
            if len > limit {
                return Poll::Ready(Err(UrlencodedError::Overflow { size: len, limit }));
            }
        }

        // future
        let encoding = self.encoding;
        let mut stream = self.stream.take().unwrap();

        self.fut = Some(
            async move {
                let mut body = BytesMut::with_capacity(8192);

                while let Some(item) = stream.next().await {
                    let chunk = item?;

                    if (body.len() + chunk.len()) > limit {
                        return Err(UrlencodedError::Overflow {
                            size: body.len() + chunk.len(),
                            limit,
                        });
                    } else {
                        body.extend_from_slice(&chunk);
                    }
                }

                if encoding == UTF_8 {
                    serde_urlencoded::from_bytes::<T>(&body).map_err(UrlencodedError::Parse)
                } else {
                    let body = encoding
                        .decode_without_bom_handling_and_without_replacement(&body)
                        .map(Cow::into_owned)
                        .ok_or(UrlencodedError::Encoding)?;

                    serde_urlencoded::from_str::<T>(&body).map_err(UrlencodedError::Parse)
                }
            }
            .boxed_local(),
        );

        self.poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::http::{
        header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE},
        StatusCode,
    };
    use crate::test::TestRequest;

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct Info {
        hello: String,
        counter: i64,
    }

    #[actix_rt::test]
    async fn test_form() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((CONTENT_LENGTH, 11))
            .set_payload(Bytes::from_static(b"hello=world&counter=123"))
            .to_http_parts();

        let Form(s) = Form::<Info>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(
            s,
            Info {
                hello: "world".into(),
                counter: 123
            }
        );
    }

    fn eq(err: UrlencodedError, other: UrlencodedError) -> bool {
        match err {
            UrlencodedError::Overflow { .. } => {
                matches!(other, UrlencodedError::Overflow { .. })
            }
            UrlencodedError::UnknownLength => matches!(other, UrlencodedError::UnknownLength),
            UrlencodedError::ContentType => matches!(other, UrlencodedError::ContentType),
            _ => false,
        }
    }

    #[actix_rt::test]
    async fn test_urlencoded_error() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((CONTENT_LENGTH, "xxxx"))
            .to_http_parts();
        let info = UrlEncoded::<Info>::new(&req, &mut pl).await;
        assert!(eq(info.err().unwrap(), UrlencodedError::UnknownLength));

        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((CONTENT_LENGTH, "1000000"))
            .to_http_parts();
        let info = UrlEncoded::<Info>::new(&req, &mut pl).await;
        assert!(eq(
            info.err().unwrap(),
            UrlencodedError::Overflow { size: 0, limit: 0 }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, "text/plain"))
            .insert_header((CONTENT_LENGTH, 10))
            .to_http_parts();
        let info = UrlEncoded::<Info>::new(&req, &mut pl).await;
        assert!(eq(info.err().unwrap(), UrlencodedError::ContentType));
    }

    #[actix_rt::test]
    async fn test_urlencoded() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((CONTENT_LENGTH, 11))
            .set_payload(Bytes::from_static(b"hello=world&counter=123"))
            .to_http_parts();

        let info = UrlEncoded::<Info>::new(&req, &mut pl).await.unwrap();
        assert_eq!(
            info,
            Info {
                hello: "world".to_owned(),
                counter: 123
            }
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                CONTENT_TYPE,
                "application/x-www-form-urlencoded; charset=utf-8",
            ))
            .insert_header((CONTENT_LENGTH, 11))
            .set_payload(Bytes::from_static(b"hello=world&counter=123"))
            .to_http_parts();

        let info = UrlEncoded::<Info>::new(&req, &mut pl).await.unwrap();
        assert_eq!(
            info,
            Info {
                hello: "world".to_owned(),
                counter: 123
            }
        );
    }

    #[actix_rt::test]
    async fn test_responder() {
        let req = TestRequest::default().to_http_request();

        let form = Form(Info {
            hello: "world".to_string(),
            counter: 123,
        });
        let resp = form.respond_to(&req);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(CONTENT_TYPE).unwrap(),
            HeaderValue::from_static("application/x-www-form-urlencoded")
        );

        use crate::responder::tests::BodyTest;
        assert_eq!(resp.body().bin_ref(), b"hello=world&counter=123");
    }

    #[actix_rt::test]
    async fn test_with_config_in_data_wrapper() {
        let ctype = HeaderValue::from_static("application/x-www-form-urlencoded");

        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, ctype))
            .insert_header((CONTENT_LENGTH, HeaderValue::from_static("20")))
            .set_payload(Bytes::from_static(b"hello=test&counter=4"))
            .app_data(web::Data::new(FormConfig::default().limit(10)))
            .to_http_parts();

        let s = Form::<Info>::from_request(&req, &mut pl).await;
        assert!(s.is_err());

        let err_str = s.err().unwrap().to_string();
        assert!(err_str.starts_with("URL encoded payload is larger"));
    }
}
