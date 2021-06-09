//! Various helpers for Actix applications to use during testing.

use std::{borrow::Cow, net::SocketAddr, rc::Rc};

pub use actix_http::test::TestBuffer;
use actix_http::{
    body,
    http::{header::IntoHeaderPair, Method, StatusCode, Uri, Version},
    test::TestRequest as HttpTestRequest,
    Extensions, Request,
};
use actix_router::{Path, ResourceDef, Url};
use actix_service::{IntoService, IntoServiceFactory, Service, ServiceFactory};
use actix_utils::future::{ok, poll_fn};
use futures_core::Stream;
use futures_util::StreamExt as _;
use serde::{de::DeserializeOwned, Serialize};

#[cfg(feature = "cookies")]
use crate::cookie::{Cookie, CookieJar};
use crate::{
    app_service::AppInitServiceState,
    config::AppConfig,
    data::Data,
    dev::{Body, MessageBody, Payload},
    http::header::ContentType,
    rmap::ResourceMap,
    service::{ServiceRequest, ServiceResponse},
    web::{Bytes, BytesMut},
    Error, HttpRequest, HttpResponse, HttpResponseBuilder,
};

/// Create service that always responds with `HttpResponse::Ok()` and no body.
pub fn ok_service(
) -> impl Service<ServiceRequest, Response = ServiceResponse<Body>, Error = Error> {
    default_service(StatusCode::OK)
}

/// Create service that always responds with given status code and no body.
pub fn default_service(
    status_code: StatusCode,
) -> impl Service<ServiceRequest, Response = ServiceResponse<Body>, Error = Error> {
    (move |req: ServiceRequest| {
        ok(req.into_response(HttpResponseBuilder::new(status_code).finish()))
    })
    .into_service()
}

/// Initialize service from application builder instance.
///
/// ```
/// use actix_service::Service;
/// use actix_web::{test, web, App, HttpResponse, http::StatusCode};
///
/// #[actix_rt::test]
/// async fn test_init_service() {
///     let app = test::init_service(
///         App::new()
///             .service(web::resource("/test").to(|| async { HttpResponse::Ok() }))
///     ).await;
///
///     // Create request object
///     let req = test::TestRequest::with_uri("/test").to_request();
///
///     // Execute application
///     let resp = app.call(req).await.unwrap();
///     assert_eq!(resp.status(), StatusCode::OK);
/// }
/// ```
pub async fn init_service<R, S, B, E>(
    app: R,
) -> impl Service<Request, Response = ServiceResponse<B>, Error = E>
where
    R: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig, Response = ServiceResponse<B>, Error = E>,
    S::InitError: std::fmt::Debug,
{
    try_init_service(app)
        .await
        .expect("service initialization failed")
}

/// Fallible version of [`init_service`] that allows testing initialization errors.
pub(crate) async fn try_init_service<R, S, B, E>(
    app: R,
) -> Result<impl Service<Request, Response = ServiceResponse<B>, Error = E>, S::InitError>
where
    R: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig, Response = ServiceResponse<B>, Error = E>,
    S::InitError: std::fmt::Debug,
{
    let srv = app.into_factory();
    srv.new_service(AppConfig::default()).await
}

/// Calls service and waits for response future completion.
///
/// ```
/// use actix_web::{test, web, App, HttpResponse, http::StatusCode};
///
/// #[actix_rt::test]
/// async fn test_response() {
///     let app = test::init_service(
///         App::new()
///             .service(web::resource("/test").to(|| async {
///                 HttpResponse::Ok()
///             }))
///     ).await;
///
///     // Create request object
///     let req = test::TestRequest::with_uri("/test").to_request();
///
///     // Call application
///     let resp = test::call_service(&app, req).await;
///     assert_eq!(resp.status(), StatusCode::OK);
/// }
/// ```
pub async fn call_service<S, R, B, E>(app: &S, req: R) -> S::Response
where
    S: Service<R, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
{
    app.call(req).await.unwrap()
}

/// Helper function that returns a response body of a TestRequest
///
/// ```
/// use actix_web::{test, web, App, HttpResponse, http::header};
/// use bytes::Bytes;
///
/// #[actix_rt::test]
/// async fn test_index() {
///     let app = test::init_service(
///         App::new().service(
///             web::resource("/index.html")
///                 .route(web::post().to(|| async {
///                     HttpResponse::Ok().body("welcome!")
///                 })))
///     ).await;
///
///     let req = test::TestRequest::post()
///         .uri("/index.html")
///         .header(header::CONTENT_TYPE, "application/json")
///         .to_request();
///
///     let result = test::read_response(&app, req).await;
///     assert_eq!(result, Bytes::from_static(b"welcome!"));
/// }
/// ```
pub async fn read_response<S, B>(app: &S, req: Request) -> Bytes
where
    S: Service<Request, Response = ServiceResponse<B>, Error = Error>,
    B: MessageBody + Unpin,
    B::Error: Into<Error>,
{
    let resp = app
        .call(req)
        .await
        .unwrap_or_else(|e| panic!("read_response failed at application call: {}", e));

    let body = resp.into_body();
    let mut bytes = BytesMut::new();

    actix_rt::pin!(body);
    while let Some(item) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        bytes.extend_from_slice(&item.map_err(Into::into).unwrap());
    }

    bytes.freeze()
}

/// Helper function that returns a response body of a ServiceResponse.
///
/// ```
/// use actix_web::{test, web, App, HttpResponse, http::header};
/// use bytes::Bytes;
///
/// #[actix_rt::test]
/// async fn test_index() {
///     let app = test::init_service(
///         App::new().service(
///             web::resource("/index.html")
///                 .route(web::post().to(|| async {
///                     HttpResponse::Ok().body("welcome!")
///                 })))
///     ).await;
///
///     let req = test::TestRequest::post()
///         .uri("/index.html")
///         .header(header::CONTENT_TYPE, "application/json")
///         .to_request();
///
///     let resp = test::call_service(&app, req).await;
///     let result = test::read_body(resp).await;
///     assert_eq!(result, Bytes::from_static(b"welcome!"));
/// }
/// ```
pub async fn read_body<B>(res: ServiceResponse<B>) -> Bytes
where
    B: MessageBody + Unpin,
    B::Error: Into<Error>,
{
    let body = res.into_body();
    let mut bytes = BytesMut::new();

    actix_rt::pin!(body);
    while let Some(item) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        bytes.extend_from_slice(&item.map_err(Into::into).unwrap());
    }

    bytes.freeze()
}

/// Helper function that returns a deserialized response body of a ServiceResponse.
///
/// ```
/// use actix_web::{App, test, web, HttpResponse, http::header};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize)]
/// pub struct Person {
///     id: String,
///     name: String,
/// }
///
/// #[actix_rt::test]
/// async fn test_post_person() {
///     let app = test::init_service(
///         App::new().service(
///             web::resource("/people")
///                 .route(web::post().to(|person: web::Json<Person>| async {
///                     HttpResponse::Ok()
///                         .json(person)})
///                     ))
///     ).await;
///
///     let payload = r#"{"id":"12345","name":"User name"}"#.as_bytes();
///
///     let resp = test::TestRequest::post()
///         .uri("/people")
///         .header(header::CONTENT_TYPE, "application/json")
///         .set_payload(payload)
///         .send_request(&mut app)
///         .await;
///
///     assert!(resp.status().is_success());
///
///     let result: Person = test::read_body_json(resp).await;
/// }
/// ```
pub async fn read_body_json<T, B>(res: ServiceResponse<B>) -> T
where
    B: MessageBody + Unpin,
    B::Error: Into<Error>,
    T: DeserializeOwned,
{
    let body = read_body(res).await;

    serde_json::from_slice(&body).unwrap_or_else(|e| {
        panic!(
            "read_response_json failed during deserialization of body: {:?}, {}",
            body, e
        )
    })
}

pub async fn load_stream<S>(mut stream: S) -> Result<Bytes, Error>
where
    S: Stream<Item = Result<Bytes, Error>> + Unpin,
{
    let mut data = BytesMut::new();
    while let Some(item) = stream.next().await {
        data.extend_from_slice(&item?);
    }
    Ok(data.freeze())
}

pub async fn load_body<B>(body: B) -> Result<Bytes, Error>
where
    B: MessageBody + Unpin,
    B::Error: Into<Error>,
{
    body::to_bytes(body).await.map_err(Into::into)
}

/// Helper function that returns a deserialized response body of a TestRequest
///
/// ```
/// use actix_web::{App, test, web, HttpResponse, http::header};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize)]
/// pub struct Person {
///     id: String,
///     name: String
/// }
///
/// #[actix_rt::test]
/// async fn test_add_person() {
///     let app = test::init_service(
///         App::new().service(
///             web::resource("/people")
///                 .route(web::post().to(|person: web::Json<Person>| async {
///                     HttpResponse::Ok()
///                         .json(person)})
///                     ))
///     ).await;
///
///     let payload = r#"{"id":"12345","name":"User name"}"#.as_bytes();
///
///     let req = test::TestRequest::post()
///         .uri("/people")
///         .header(header::CONTENT_TYPE, "application/json")
///         .set_payload(payload)
///         .to_request();
///
///     let result: Person = test::read_response_json(&mut app, req).await;
/// }
/// ```
pub async fn read_response_json<S, B, T>(app: &S, req: Request) -> T
where
    S: Service<Request, Response = ServiceResponse<B>, Error = Error>,
    B: MessageBody + Unpin,
    B::Error: Into<Error>,
    T: DeserializeOwned,
{
    let body = read_response(app, req).await;

    serde_json::from_slice(&body).unwrap_or_else(|_| {
        panic!(
            "read_response_json failed during deserialization of body: {:?}",
            body
        )
    })
}

/// Test `Request` builder.
///
/// For unit testing, actix provides a request builder type and a simple handler runner. TestRequest implements a builder-like pattern.
/// You can generate various types of request via TestRequest's methods:
///  * `TestRequest::to_request` creates `actix_http::Request` instance.
///  * `TestRequest::to_srv_request` creates `ServiceRequest` instance, which is used for testing middlewares and chain adapters.
///  * `TestRequest::to_srv_response` creates `ServiceResponse` instance.
///  * `TestRequest::to_http_request` creates `HttpRequest` instance, which is used for testing handlers.
///
/// ```
/// use actix_web::{test, HttpRequest, HttpResponse, HttpMessage};
/// use actix_web::http::{header, StatusCode};
///
/// async fn index(req: HttpRequest) -> HttpResponse {
///     if let Some(hdr) = req.headers().get(header::CONTENT_TYPE) {
///         HttpResponse::Ok().into()
///     } else {
///         HttpResponse::BadRequest().into()
///     }
/// }
///
/// #[test]
/// fn test_index() {
///     let req = test::TestRequest::default().insert_header("content-type", "text/plain")
///         .to_http_request();
///
///     let resp = index(req).await.unwrap();
///     assert_eq!(resp.status(), StatusCode::OK);
///
///     let req = test::TestRequest::default().to_http_request();
///     let resp = index(req).await.unwrap();
///     assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
/// }
/// ```
pub struct TestRequest {
    req: HttpTestRequest,
    rmap: ResourceMap,
    config: AppConfig,
    path: Path<Url>,
    peer_addr: Option<SocketAddr>,
    app_data: Extensions,
    #[cfg(feature = "cookies")]
    cookies: CookieJar,
}

impl Default for TestRequest {
    fn default() -> TestRequest {
        TestRequest {
            req: HttpTestRequest::default(),
            rmap: ResourceMap::new(ResourceDef::new("")),
            config: AppConfig::default(),
            path: Path::new(Url::new(Uri::default())),
            peer_addr: None,
            app_data: Extensions::new(),
            #[cfg(feature = "cookies")]
            cookies: CookieJar::new(),
        }
    }
}

#[allow(clippy::wrong_self_convention)]
impl TestRequest {
    /// Create TestRequest and set request uri
    pub fn with_uri(path: &str) -> TestRequest {
        TestRequest::default().uri(path)
    }

    /// Create TestRequest and set method to `Method::GET`
    pub fn get() -> TestRequest {
        TestRequest::default().method(Method::GET)
    }

    /// Create TestRequest and set method to `Method::POST`
    pub fn post() -> TestRequest {
        TestRequest::default().method(Method::POST)
    }

    /// Create TestRequest and set method to `Method::PUT`
    pub fn put() -> TestRequest {
        TestRequest::default().method(Method::PUT)
    }

    /// Create TestRequest and set method to `Method::PATCH`
    pub fn patch() -> TestRequest {
        TestRequest::default().method(Method::PATCH)
    }

    /// Create TestRequest and set method to `Method::DELETE`
    pub fn delete() -> TestRequest {
        TestRequest::default().method(Method::DELETE)
    }

    /// Set HTTP version of this request
    pub fn version(mut self, ver: Version) -> Self {
        self.req.version(ver);
        self
    }

    /// Set HTTP method of this request
    pub fn method(mut self, meth: Method) -> Self {
        self.req.method(meth);
        self
    }

    /// Set HTTP Uri of this request
    pub fn uri(mut self, path: &str) -> Self {
        self.req.uri(path);
        self
    }

    /// Insert a header, replacing any that were set with an equivalent field name.
    pub fn insert_header<H>(mut self, header: H) -> Self
    where
        H: IntoHeaderPair,
    {
        self.req.insert_header(header);
        self
    }

    /// Append a header, keeping any that were set with an equivalent field name.
    pub fn append_header<H>(mut self, header: H) -> Self
    where
        H: IntoHeaderPair,
    {
        self.req.append_header(header);
        self
    }

    /// Set cookie for this request.
    #[cfg(feature = "cookies")]
    pub fn cookie(mut self, cookie: Cookie<'_>) -> Self {
        self.cookies.add(cookie.into_owned());
        self
    }

    /// Set request path pattern parameter
    pub fn param(
        mut self,
        name: impl Into<Cow<'static, str>>,
        value: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.path.add_static(name, value);
        self
    }

    /// Set peer addr
    pub fn peer_addr(mut self, addr: SocketAddr) -> Self {
        self.peer_addr = Some(addr);
        self
    }

    /// Set request payload
    pub fn set_payload<B: Into<Bytes>>(mut self, data: B) -> Self {
        self.req.set_payload(data);
        self
    }

    /// Serialize `data` to a URL encoded form and set it as the request payload. The `Content-Type`
    /// header is set to `application/x-www-form-urlencoded`.
    pub fn set_form<T: Serialize>(mut self, data: &T) -> Self {
        let bytes = serde_urlencoded::to_string(data)
            .expect("Failed to serialize test data as a urlencoded form");
        self.req.set_payload(bytes);
        self.req.insert_header(ContentType::form_url_encoded());
        self
    }

    /// Serialize `data` to JSON and set it as the request payload. The `Content-Type` header is
    /// set to `application/json`.
    pub fn set_json<T: Serialize>(mut self, data: &T) -> Self {
        let bytes = serde_json::to_string(data).expect("Failed to serialize test data to json");
        self.req.set_payload(bytes);
        self.req.insert_header(ContentType::json());
        self
    }

    /// Set application data. This is equivalent of `App::data()` method
    /// for testing purpose.
    pub fn data<T: 'static>(mut self, data: T) -> Self {
        self.app_data.insert(Data::new(data));
        self
    }

    /// Set application data. This is equivalent of `App::app_data()` method
    /// for testing purpose.
    pub fn app_data<T: 'static>(mut self, data: T) -> Self {
        self.app_data.insert(data);
        self
    }

    #[cfg(test)]
    /// Set request config
    pub(crate) fn rmap(mut self, rmap: ResourceMap) -> Self {
        self.rmap = rmap;
        self
    }

    fn finish(&mut self) -> Request {
        // mut used when cookie feature is enabled
        #[allow(unused_mut)]
        let mut req = self.req.finish();

        #[cfg(feature = "cookies")]
        {
            use actix_http::http::header::{HeaderValue, COOKIE};

            let cookie: String = self
                .cookies
                .delta()
                // ensure only name=value is written to cookie header
                .map(|c| c.stripped().encoded().to_string())
                .collect::<Vec<_>>()
                .join("; ");

            if !cookie.is_empty() {
                req.headers_mut()
                    .insert(COOKIE, HeaderValue::from_str(&cookie).unwrap());
            }
        }

        req
    }

    /// Complete request creation and generate `Request` instance
    pub fn to_request(mut self) -> Request {
        let mut req = self.finish();
        req.head_mut().peer_addr = self.peer_addr;
        req
    }

    /// Complete request creation and generate `ServiceRequest` instance
    pub fn to_srv_request(mut self) -> ServiceRequest {
        let (mut head, payload) = self.finish().into_parts();
        head.peer_addr = self.peer_addr;
        self.path.get_mut().update(&head.uri);

        let app_state = AppInitServiceState::new(Rc::new(self.rmap), self.config.clone());

        ServiceRequest::new(
            HttpRequest::new(self.path, head, app_state, Rc::new(self.app_data)),
            payload,
        )
    }

    /// Complete request creation and generate `ServiceResponse` instance
    pub fn to_srv_response<B>(self, res: HttpResponse<B>) -> ServiceResponse<B> {
        self.to_srv_request().into_response(res)
    }

    /// Complete request creation and generate `HttpRequest` instance
    pub fn to_http_request(mut self) -> HttpRequest {
        let (mut head, _) = self.finish().into_parts();
        head.peer_addr = self.peer_addr;
        self.path.get_mut().update(&head.uri);

        let app_state = AppInitServiceState::new(Rc::new(self.rmap), self.config.clone());

        HttpRequest::new(self.path, head, app_state, Rc::new(self.app_data))
    }

    /// Complete request creation and generate `HttpRequest` and `Payload` instances
    pub fn to_http_parts(mut self) -> (HttpRequest, Payload) {
        let (mut head, payload) = self.finish().into_parts();
        head.peer_addr = self.peer_addr;
        self.path.get_mut().update(&head.uri);

        let app_state = AppInitServiceState::new(Rc::new(self.rmap), self.config.clone());

        let req = HttpRequest::new(self.path, head, app_state, Rc::new(self.app_data));

        (req, payload)
    }

    /// Complete request creation, calls service and waits for response future completion.
    pub async fn send_request<S, B, E>(self, app: &S) -> S::Response
    where
        S: Service<Request, Response = ServiceResponse<B>, Error = E>,
        E: std::fmt::Debug,
    {
        let req = self.to_request();
        call_service(app, req).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use actix_http::HttpMessage;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{http::header, web, App, HttpResponse, Responder};

    #[actix_rt::test]
    async fn test_basics() {
        let req = TestRequest::default()
            .version(Version::HTTP_2)
            .insert_header(header::ContentType::json())
            .insert_header(header::Date(SystemTime::now().into()))
            .param("test", "123")
            .data(10u32)
            .app_data(20u64)
            .peer_addr("127.0.0.1:8081".parse().unwrap())
            .to_http_request();
        assert!(req.headers().contains_key(header::CONTENT_TYPE));
        assert!(req.headers().contains_key(header::DATE));
        assert_eq!(
            req.head().peer_addr,
            Some("127.0.0.1:8081".parse().unwrap())
        );
        assert_eq!(&req.match_info()["test"], "123");
        assert_eq!(req.version(), Version::HTTP_2);
        let data = req.app_data::<Data<u32>>().unwrap();
        assert!(req.app_data::<Data<u64>>().is_none());
        assert_eq!(*data.get_ref(), 10);

        assert!(req.app_data::<u32>().is_none());
        let data = req.app_data::<u64>().unwrap();
        assert_eq!(*data, 20);
    }

    #[actix_rt::test]
    async fn test_request_methods() {
        let app = init_service(
            App::new().service(
                web::resource("/index.html")
                    .route(web::put().to(|| HttpResponse::Ok().body("put!")))
                    .route(web::patch().to(|| HttpResponse::Ok().body("patch!")))
                    .route(web::delete().to(|| HttpResponse::Ok().body("delete!"))),
            ),
        )
        .await;

        let put_req = TestRequest::put()
            .uri("/index.html")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .to_request();

        let result = read_response(&app, put_req).await;
        assert_eq!(result, Bytes::from_static(b"put!"));

        let patch_req = TestRequest::patch()
            .uri("/index.html")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .to_request();

        let result = read_response(&app, patch_req).await;
        assert_eq!(result, Bytes::from_static(b"patch!"));

        let delete_req = TestRequest::delete().uri("/index.html").to_request();
        let result = read_response(&app, delete_req).await;
        assert_eq!(result, Bytes::from_static(b"delete!"));
    }

    #[actix_rt::test]
    async fn test_response() {
        let app = init_service(
            App::new().service(
                web::resource("/index.html")
                    .route(web::post().to(|| HttpResponse::Ok().body("welcome!"))),
            ),
        )
        .await;

        let req = TestRequest::post()
            .uri("/index.html")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .to_request();

        let result = read_response(&app, req).await;
        assert_eq!(result, Bytes::from_static(b"welcome!"));
    }

    #[actix_rt::test]
    async fn test_send_request() {
        let app = init_service(
            App::new().service(
                web::resource("/index.html")
                    .route(web::get().to(|| HttpResponse::Ok().body("welcome!"))),
            ),
        )
        .await;

        let resp = TestRequest::get()
            .uri("/index.html")
            .send_request(&app)
            .await;

        let result = read_body(resp).await;
        assert_eq!(result, Bytes::from_static(b"welcome!"));
    }

    #[derive(Serialize, Deserialize)]
    pub struct Person {
        id: String,
        name: String,
    }

    #[actix_rt::test]
    async fn test_response_json() {
        let app = init_service(App::new().service(web::resource("/people").route(
            web::post().to(|person: web::Json<Person>| HttpResponse::Ok().json(person)),
        )))
        .await;

        let payload = r#"{"id":"12345","name":"User name"}"#.as_bytes();

        let req = TestRequest::post()
            .uri("/people")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .set_payload(payload)
            .to_request();

        let result: Person = read_response_json(&app, req).await;
        assert_eq!(&result.id, "12345");
    }

    #[actix_rt::test]
    async fn test_body_json() {
        let app = init_service(App::new().service(web::resource("/people").route(
            web::post().to(|person: web::Json<Person>| HttpResponse::Ok().json(person)),
        )))
        .await;

        let payload = r#"{"id":"12345","name":"User name"}"#.as_bytes();

        let resp = TestRequest::post()
            .uri("/people")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .set_payload(payload)
            .send_request(&app)
            .await;

        let result: Person = read_body_json(resp).await;
        assert_eq!(&result.name, "User name");
    }

    #[actix_rt::test]
    async fn test_request_response_form() {
        let app = init_service(App::new().service(web::resource("/people").route(
            web::post().to(|person: web::Form<Person>| HttpResponse::Ok().json(person)),
        )))
        .await;

        let payload = Person {
            id: "12345".to_string(),
            name: "User name".to_string(),
        };

        let req = TestRequest::post()
            .uri("/people")
            .set_form(&payload)
            .to_request();

        assert_eq!(req.content_type(), "application/x-www-form-urlencoded");

        let result: Person = read_response_json(&app, req).await;
        assert_eq!(&result.id, "12345");
        assert_eq!(&result.name, "User name");
    }

    #[actix_rt::test]
    async fn test_request_response_json() {
        let app = init_service(App::new().service(web::resource("/people").route(
            web::post().to(|person: web::Json<Person>| HttpResponse::Ok().json(person)),
        )))
        .await;

        let payload = Person {
            id: "12345".to_string(),
            name: "User name".to_string(),
        };

        let req = TestRequest::post()
            .uri("/people")
            .set_json(&payload)
            .to_request();

        assert_eq!(req.content_type(), "application/json");

        let result: Person = read_response_json(&app, req).await;
        assert_eq!(&result.id, "12345");
        assert_eq!(&result.name, "User name");
    }

    #[actix_rt::test]
    async fn test_async_with_block() {
        async fn async_with_block() -> Result<HttpResponse, Error> {
            let res = web::block(move || Some(4usize).ok_or("wrong")).await;

            match res {
                Ok(value) => Ok(HttpResponse::Ok()
                    .content_type("text/plain")
                    .body(format!("Async with block value: {:?}", value))),
                Err(_) => panic!("Unexpected"),
            }
        }

        let app =
            init_service(App::new().service(web::resource("/index.html").to(async_with_block)))
                .await;

        let req = TestRequest::post().uri("/index.html").to_request();
        let res = app.call(req).await.unwrap();
        assert!(res.status().is_success());
    }

    #[actix_rt::test]
    async fn test_server_data() {
        async fn handler(data: web::Data<usize>) -> impl Responder {
            assert_eq!(**data, 10);
            HttpResponse::Ok()
        }

        let app = init_service(
            App::new()
                .data(10usize)
                .service(web::resource("/index.html").to(handler)),
        )
        .await;

        let req = TestRequest::post().uri("/index.html").to_request();
        let res = app.call(req).await.unwrap();
        assert!(res.status().is_success());
    }
}
