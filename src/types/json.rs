//! For JSON helper documentation, see [`Json`].

use std::{
    fmt,
    future::Future,
    marker::PhantomData,
    ops,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::BytesMut;
use futures_core::{ready, stream::Stream as _};
use serde::{de::DeserializeOwned, Serialize};

use actix_http::Payload;

#[cfg(feature = "__compress")]
use crate::dev::Decompress;
use crate::{
    error::{Error, JsonPayloadError},
    extract::FromRequest,
    http::header::CONTENT_LENGTH,
    request::HttpRequest,
    web, HttpMessage, HttpResponse, Responder,
};

/// JSON extractor and responder.
///
/// `Json` has two uses: JSON responses, and extracting typed data from JSON request payloads.
///
/// # Extractor
/// To extract typed data from a request body, the inner type `T` must implement the
/// [`serde::Deserialize`] trait.
///
/// Use [`JsonConfig`] to configure extraction process.
///
/// ```
/// use actix_web::{post, web, App};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     username: String,
/// }
///
/// /// deserialize `Info` from request's body
/// #[post("/")]
/// async fn index(info: web::Json<Info>) -> String {
///     format!("Welcome {}!", info.username)
/// }
/// ```
///
/// # Responder
/// The `Json` type  JSON formatted responses. A handler may return a value of type
/// `Json<T>` where `T` is the type of a structure to serialize into JSON. The type `T` must
/// implement [`serde::Serialize`].
///
/// ```
/// use actix_web::{post, web, HttpRequest};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Info {
///     name: String,
/// }
///
/// #[post("/{name}")]
/// async fn index(req: HttpRequest) -> web::Json<Info> {
///     web::Json(Info {
///         name: req.match_info().get("name").unwrap().to_owned(),
///     })
/// }
/// ```
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> fmt::Display for Json<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<T> Serialize for Json<T>
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

/// Creates response with OK status code, correct content type header, and serialized JSON payload.
///
/// If serialization failed
impl<T: Serialize> Responder for Json<T> {
    fn respond_to(self, _: &HttpRequest) -> HttpResponse {
        match serde_json::to_string(&self.0) {
            Ok(body) => HttpResponse::Ok()
                .content_type(mime::APPLICATION_JSON)
                .body(body),
            Err(err) => HttpResponse::from_error(JsonPayloadError::Serialize(err)),
        }
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<T> FromRequest for Json<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = Error;
    type Future = JsonExtractFut<T>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let config = JsonConfig::from_req(req);

        let limit = config.limit;
        let ctype = config.content_type.as_deref();
        let err_handler = config.err_handler.clone();

        JsonExtractFut {
            req: Some(req.clone()),
            fut: JsonBody::new(req, payload, ctype).limit(limit),
            err_handler,
        }
    }
}

type JsonErrorHandler =
    Option<Arc<dyn Fn(JsonPayloadError, &HttpRequest) -> Error + Send + Sync>>;

pub struct JsonExtractFut<T> {
    req: Option<HttpRequest>,
    fut: JsonBody<T>,
    err_handler: JsonErrorHandler,
}

impl<T> Future for JsonExtractFut<T>
where
    T: DeserializeOwned + 'static,
{
    type Output = Result<Json<T>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let res = ready!(Pin::new(&mut this.fut).poll(cx));

        let res = match res {
            Err(err) => {
                let req = this.req.take().unwrap();
                log::debug!(
                    "Failed to deserialize Json from payload. \
                         Request path: {}",
                    req.path()
                );

                if let Some(err_handler) = this.err_handler.as_ref() {
                    Err((*err_handler)(err, &req))
                } else {
                    Err(err.into())
                }
            }
            Ok(data) => Ok(Json(data)),
        };

        Poll::Ready(res)
    }
}

/// `Json` extractor configuration.
///
/// # Examples
/// ```
/// use actix_web::{error, post, web, App, FromRequest, HttpResponse};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     name: String,
/// }
///
/// // `Json` extraction is bound by custom `JsonConfig` applied to App.
/// #[post("/")]
/// async fn index(info: web::Json<Info>) -> String {
///     format!("Welcome {}!", info.name)
/// }
///
/// // custom `Json` extractor configuration
/// let json_cfg = web::JsonConfig::default()
///     // limit request payload size
///     .limit(4096)
///     // only accept text/plain content type
///     .content_type(|mime| mime == mime::TEXT_PLAIN)
///     // use custom error handler
///     .error_handler(|err, req| {
///         error::InternalError::from_response(err, HttpResponse::Conflict().into()).into()
///     });
///
/// App::new()
///     .app_data(json_cfg)
///     .service(index);
/// ```
#[derive(Clone)]
pub struct JsonConfig {
    limit: usize,
    err_handler: JsonErrorHandler,
    content_type: Option<Arc<dyn Fn(mime::Mime) -> bool + Send + Sync>>,
}

impl JsonConfig {
    /// Set maximum accepted payload size. By default this limit is 2MB.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set custom error handler.
    pub fn error_handler<F>(mut self, f: F) -> Self
    where
        F: Fn(JsonPayloadError, &HttpRequest) -> Error + Send + Sync + 'static,
    {
        self.err_handler = Some(Arc::new(f));
        self
    }

    /// Set predicate for allowed content types.
    pub fn content_type<F>(mut self, predicate: F) -> Self
    where
        F: Fn(mime::Mime) -> bool + Send + Sync + 'static,
    {
        self.content_type = Some(Arc::new(predicate));
        self
    }

    /// Extract payload config from app data. Check both `T` and `Data<T>`, in that order, and fall
    /// back to the default payload config.
    fn from_req(req: &HttpRequest) -> &Self {
        req.app_data::<Self>()
            .or_else(|| req.app_data::<web::Data<Self>>().map(|d| d.as_ref()))
            .unwrap_or(&DEFAULT_CONFIG)
    }
}

const DEFAULT_LIMIT: usize = 2_097_152; // 2 mb

/// Allow shared refs used as default.
const DEFAULT_CONFIG: JsonConfig = JsonConfig {
    limit: DEFAULT_LIMIT,
    err_handler: None,
    content_type: None,
};

impl Default for JsonConfig {
    fn default() -> Self {
        DEFAULT_CONFIG.clone()
    }
}

/// Future that resolves to some `T` when parsed from a JSON payload.
///
/// Form can be deserialized from any type `T` that implements [`serde::Deserialize`].
///
/// Returns error if:
/// - content type is not `application/json`
/// - content length is greater than [limit](JsonBody::limit())
pub enum JsonBody<T> {
    Error(Option<JsonPayloadError>),
    Body {
        limit: usize,
        length: Option<usize>,
        #[cfg(feature = "__compress")]
        payload: Decompress<Payload>,
        #[cfg(not(feature = "__compress"))]
        payload: Payload,
        buf: BytesMut,
        _res: PhantomData<T>,
    },
}

impl<T> Unpin for JsonBody<T> {}

impl<T> JsonBody<T>
where
    T: DeserializeOwned + 'static,
{
    /// Create a new future to decode a JSON request payload.
    #[allow(clippy::borrow_interior_mutable_const)]
    pub fn new(
        req: &HttpRequest,
        payload: &mut Payload,
        ctype: Option<&(dyn Fn(mime::Mime) -> bool + Send + Sync)>,
    ) -> Self {
        // check content-type
        let json = if let Ok(Some(mime)) = req.mime_type() {
            mime.subtype() == mime::JSON
                || mime.suffix() == Some(mime::JSON)
                || ctype.map_or(false, |predicate| predicate(mime))
        } else {
            false
        };

        if !json {
            return JsonBody::Error(Some(JsonPayloadError::ContentType));
        }

        let length = req
            .headers()
            .get(&CONTENT_LENGTH)
            .and_then(|l| l.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        // Notice the content_length is not checked against limit of json config here.
        // As the internal usage always call JsonBody::limit after JsonBody::new.
        // And limit check to return an error variant of JsonBody happens there.

        let payload = {
            cfg_if::cfg_if! {
                if #[cfg(feature = "__compress")] {
                    Decompress::from_headers(payload.take(), req.headers())
                } else {
                    payload.take()
                }
            }
        };

        JsonBody::Body {
            limit: DEFAULT_LIMIT,
            length,
            payload,
            buf: BytesMut::with_capacity(8192),
            _res: PhantomData,
        }
    }

    /// Set maximum accepted payload size. The default limit is 2MB.
    pub fn limit(self, limit: usize) -> Self {
        match self {
            JsonBody::Body {
                length,
                payload,
                buf,
                ..
            } => {
                if let Some(len) = length {
                    if len > limit {
                        return JsonBody::Error(Some(JsonPayloadError::OverflowKnownLength {
                            length: len,
                            limit,
                        }));
                    }
                }

                JsonBody::Body {
                    limit,
                    length,
                    payload,
                    buf,
                    _res: PhantomData,
                }
            }
            JsonBody::Error(e) => JsonBody::Error(e),
        }
    }
}

impl<T> Future for JsonBody<T>
where
    T: DeserializeOwned + 'static,
{
    type Output = Result<T, JsonPayloadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this {
            JsonBody::Body {
                limit,
                buf,
                payload,
                ..
            } => loop {
                let res = ready!(Pin::new(&mut *payload).poll_next(cx));
                match res {
                    Some(chunk) => {
                        let chunk = chunk?;
                        let buf_len = buf.len() + chunk.len();
                        if buf_len > *limit {
                            return Poll::Ready(Err(JsonPayloadError::Overflow {
                                limit: *limit,
                            }));
                        } else {
                            buf.extend_from_slice(&chunk);
                        }
                    }
                    None => {
                        let json = serde_json::from_slice::<T>(&buf)
                            .map_err(JsonPayloadError::Deserialize)?;
                        return Poll::Ready(Ok(json));
                    }
                }
            },
            JsonBody::Error(e) => Poll::Ready(Err(e.take().unwrap())),
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{
        error::InternalError,
        http::{
            header::{self, CONTENT_LENGTH, CONTENT_TYPE},
            StatusCode,
        },
        test::{load_body, TestRequest},
    };

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct MyObject {
        name: String,
    }

    fn json_eq(err: JsonPayloadError, other: JsonPayloadError) -> bool {
        match err {
            JsonPayloadError::Overflow { .. } => {
                matches!(other, JsonPayloadError::Overflow { .. })
            }
            JsonPayloadError::OverflowKnownLength { .. } => {
                matches!(other, JsonPayloadError::OverflowKnownLength { .. })
            }
            JsonPayloadError::ContentType => matches!(other, JsonPayloadError::ContentType),
            _ => false,
        }
    }

    #[actix_rt::test]
    async fn test_responder() {
        let req = TestRequest::default().to_http_request();

        let j = Json(MyObject {
            name: "test".to_string(),
        });
        let resp = j.respond_to(&req);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            header::HeaderValue::from_static("application/json")
        );

        use crate::responder::tests::BodyTest;
        assert_eq!(resp.body().bin_ref(), b"{\"name\":\"test\"}");
    }

    #[actix_rt::test]
    async fn test_custom_error_responder() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(JsonConfig::default().limit(10).error_handler(|err, _| {
                let msg = MyObject {
                    name: "invalid request".to_string(),
                };
                let resp =
                    HttpResponse::BadRequest().body(serde_json::to_string(&msg).unwrap());
                InternalError::from_response(err, resp).into()
            }))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        let resp = HttpResponse::from_error(s.err().unwrap());
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = load_body(resp.into_body()).await.unwrap();
        let msg: MyObject = serde_json::from_slice(&body).unwrap();
        assert_eq!(msg.name, "invalid request");
    }

    #[actix_rt::test]
    async fn test_extract() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.name, "test");
        assert_eq!(
            s.into_inner(),
            MyObject {
                name: "test".to_string()
            }
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(JsonConfig::default().limit(10))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        assert!(format!("{}", s.err().unwrap())
            .contains("JSON payload (16 bytes) is larger than allowed (limit: 10 bytes)."));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(
                JsonConfig::default()
                    .limit(10)
                    .error_handler(|_, _| JsonPayloadError::ContentType.into()),
            )
            .to_http_parts();
        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        assert!(format!("{}", s.err().unwrap()).contains("Content type error"));
    }

    #[actix_rt::test]
    async fn test_json_body() {
        let (req, mut pl) = TestRequest::default().to_http_parts();
        let json = JsonBody::<MyObject>::new(&req, &mut pl, None).await;
        assert!(json_eq(json.err().unwrap(), JsonPayloadError::ContentType));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/text"),
            ))
            .to_http_parts();
        let json = JsonBody::<MyObject>::new(&req, &mut pl, None).await;
        assert!(json_eq(json.err().unwrap(), JsonPayloadError::ContentType));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("10000"),
            ))
            .to_http_parts();

        let json = JsonBody::<MyObject>::new(&req, &mut pl, None)
            .limit(100)
            .await;
        assert!(json_eq(
            json.err().unwrap(),
            JsonPayloadError::OverflowKnownLength {
                length: 10000,
                limit: 100
            }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .set_payload(Bytes::from_static(&[0u8; 1000]))
            .to_http_parts();

        let json = JsonBody::<MyObject>::new(&req, &mut pl, None)
            .limit(100)
            .await;

        assert!(json_eq(
            json.err().unwrap(),
            JsonPayloadError::Overflow { limit: 100 }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let json = JsonBody::<MyObject>::new(&req, &mut pl, None).await;
        assert_eq!(
            json.ok().unwrap(),
            MyObject {
                name: "test".to_owned()
            }
        );
    }

    #[actix_rt::test]
    async fn test_with_json_and_bad_content_type() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(JsonConfig::default().limit(4096))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        assert!(s.is_err())
    }

    #[actix_rt::test]
    async fn test_with_json_and_good_custom_content_type() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(JsonConfig::default().content_type(|mime: mime::Mime| {
                mime.type_() == mime::TEXT && mime.subtype() == mime::PLAIN
            }))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        assert!(s.is_ok())
    }

    #[actix_rt::test]
    async fn test_with_json_and_bad_custom_content_type() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/html"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(JsonConfig::default().content_type(|mime: mime::Mime| {
                mime.type_() == mime::TEXT && mime.subtype() == mime::PLAIN
            }))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        assert!(s.is_err())
    }

    #[actix_rt::test]
    async fn test_with_config_in_data_wrapper() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((CONTENT_TYPE, mime::APPLICATION_JSON))
            .insert_header((CONTENT_LENGTH, 16))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .app_data(web::Data::new(JsonConfig::default().limit(10)))
            .to_http_parts();

        let s = Json::<MyObject>::from_request(&req, &mut pl).await;
        assert!(s.is_err());

        let err_str = s.err().unwrap().to_string();
        assert!(err_str
            .contains("JSON payload (16 bytes) is larger than allowed (limit: 10 bytes)."));
    }
}
