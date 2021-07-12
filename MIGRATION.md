## Unreleased

* The default `NormalizePath` behavior now strips trailing slashes by default. This was
  previously documented to be the case in v3 but the behavior now matches. The effect is that
  routes defined with trailing slashes will become inaccessible when
  using `NormalizePath::default()`.
  
  Before: `#[get("/test/")]`  
  After:  `#[get("/test")]`  

  Alternatively, explicitly require trailing slashes: `NormalizePath::new(TrailingSlash::Always)`.

* The `type Config` of `FromRequest` was removed.

* Feature flag `compress` has been split into its supported algorithm (brotli, gzip, zstd).
  By default all compression algorithms are enabled.
  To select algorithm you want to include with `middleware::Compress` use following flags:
  - `compress-brotli`
  - `compress-gzip`
  - `compress-zstd`
  If you have set in your `Cargo.toml` dedicated `actix-web` features and you still want
  to have compression enabled. Please change features selection like bellow:

  Before: `"compress"`
  After: `"compress-brotli", "compress-gzip", "compress-zstd"`


## 3.0.0

* The return type for `ServiceRequest::app_data::<T>()` was changed from returning a `Data<T>` to
  simply a `T`. To access a `Data<T>` use `ServiceRequest::app_data::<Data<T>>()`.

* Cookie handling has been offloaded to the `cookie` crate:
  * `USERINFO_ENCODE_SET` is no longer exposed. Percent-encoding is still supported; check docs.
  * Some types now require lifetime parameters.

* The time crate was updated to `v0.2`, a major breaking change to the time crate, which affects
  any `actix-web` method previously expecting a time v0.1 input.

* Setting a cookie's SameSite property, explicitly, to `SameSite::None` will now
  result in `SameSite=None` being sent with the response Set-Cookie header.
  To create a cookie without a SameSite attribute, remove any calls setting same_site.

* actix-http support for Actors messages was moved to actix-http crate and is enabled 
  with feature `actors`

* content_length function is removed from actix-http.
  You can set Content-Length by normally setting the response body or calling no_chunking function. 

* `BodySize::Sized64` variant has been removed. `BodySize::Sized` now receives a
  `u64` instead of a `usize`.

* Code that was using `path.<index>` to access a `web::Path<(A, B, C)>`s elements now needs to use
  destructuring or `.into_inner()`. For example:

  ```rust
  // Previously:
  async fn some_route(path: web::Path<(String, String)>) -> String {
    format!("Hello, {} {}", path.0, path.1)
  }

  // Now (this also worked before):
  async fn some_route(path: web::Path<(String, String)>) -> String {
    let (first_name, last_name) = path.into_inner();
    format!("Hello, {} {}", first_name, last_name)
  }
  // Or (this wasn't previously supported):
  async fn some_route(web::Path((first_name, last_name)): web::Path<(String, String)>) -> String {
    format!("Hello, {} {}", first_name, last_name)
  }
  ```

* `middleware::NormalizePath` can now also be configured to trim trailing slashes instead of always keeping one.
  It will need `middleware::normalize::TrailingSlash` when being constructed with `NormalizePath::new(...)`,
  or for an easier migration you can replace `wrap(middleware::NormalizePath)` with `wrap(middleware::NormalizePath::new(TrailingSlash::MergeOnly))`.

* `HttpServer::maxconn` is renamed to the more expressive `HttpServer::max_connections`.

* `HttpServer::maxconnrate` is renamed to the more expressive `HttpServer::max_connection_rate`.


## 2.0.0

* `HttpServer::start()` renamed to `HttpServer::run()`. It also possible to
  `.await` on `run` method result, in that case it awaits server exit.

* `App::register_data()` renamed to `App::app_data()` and accepts any type `T: 'static`.
  Stored data is available via `HttpRequest::app_data()` method at runtime.

* Extractor configuration must be registered with `App::app_data()` instead of `App::data()`

* Sync handlers has been removed. `.to_async()` method has been renamed to `.to()`
  replace `fn` with `async fn` to convert sync handler to async

* `actix_http_test::TestServer` moved to `actix_web::test` module. To start
  test server use `test::start()` or `test_start_with_config()` methods

* `ResponseError` trait has been reafctored. `ResponseError::error_response()` renders
  http response.

* Feature `rust-tls` renamed to `rustls`

  instead of

    ```rust
    actix-web = { version = "2.0.0", features = ["rust-tls"] }
    ```

  use

    ```rust
    actix-web = { version = "2.0.0", features = ["rustls"] }
    ```

* Feature `ssl` renamed to `openssl`

  instead of

    ```rust
    actix-web = { version = "2.0.0", features = ["ssl"] }
    ```

  use

    ```rust
    actix-web = { version = "2.0.0", features = ["openssl"] }
    ```
* `Cors` builder now requires that you call `.finish()` to construct the middleware

## 1.0.1

* Cors middleware has been moved to `actix-cors` crate

  instead of

  ```rust
  use actix_web::middleware::cors::Cors;
  ```

  use

  ```rust
  use actix_cors::Cors;
  ```

* Identity middleware has been moved to `actix-identity` crate

  instead of

  ```rust
  use actix_web::middleware::identity::{Identity, CookieIdentityPolicy, IdentityService};
  ```

  use

  ```rust
  use actix_identity::{Identity, CookieIdentityPolicy, IdentityService};
  ```


## 1.0.0

* Extractor configuration. In version 1.0 this is handled with the new `Data` mechanism for both setting and retrieving the configuration

  instead of

  ```rust

  #[derive(Default)]
  struct ExtractorConfig {
     config: String,
  }

  impl FromRequest for YourExtractor {
     type Config = ExtractorConfig;
     type Result = Result<YourExtractor, Error>;

     fn from_request(req: &HttpRequest, cfg: &Self::Config) -> Self::Result {
         println!("use the config: {:?}", cfg.config);
         ...
     }
  }

  App::new().resource("/route_with_config", |r| {
     r.post().with_config(handler_fn, |cfg| {
         cfg.0.config = "test".to_string();
     })
  })

  ```

  use the HttpRequest to get the configuration like any other `Data` with `req.app_data::<C>()` and set it with the `data()` method on the `resource`

  ```rust
  #[derive(Default)]
  struct ExtractorConfig {
     config: String,
  }

  impl FromRequest for YourExtractor {
     type Error = Error;
     type Future = Result<Self, Self::Error>;
     type Config = ExtractorConfig;

     fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
         let cfg = req.app_data::<ExtractorConfig>();
         println!("config data?: {:?}", cfg.unwrap().role);
         ...
     }
  }

  App::new().service(
     resource("/route_with_config")
         .data(ExtractorConfig {
             config: "test".to_string(),
         })
         .route(post().to(handler_fn)),
  )
  ```

* Resource registration. 1.0 version uses generalized resource
  registration via `.service()` method.

  instead of

  ```rust
    App.new().resource("/welcome", |r| r.f(welcome))
  ```

  use App's or Scope's `.service()` method. `.service()` method accepts
  object that implements `HttpServiceFactory` trait. By default
  actix-web provides `Resource` and `Scope` services.

  ```rust
    App.new().service(
        web::resource("/welcome")
            .route(web::get().to(welcome))
            .route(web::post().to(post_handler))
  ```

* Scope registration.

  instead of

  ```rust
      let app = App::new().scope("/{project_id}", |scope| {
            scope
                .resource("/path1", |r| r.f(|_| HttpResponse::Ok()))
                .resource("/path2", |r| r.f(|_| HttpResponse::Ok()))
                .resource("/path3", |r| r.f(|_| HttpResponse::MethodNotAllowed()))
      });
  ```

  use `.service()` for registration and `web::scope()` as scope object factory.

  ```rust
      let app = App::new().service(
          web::scope("/{project_id}")
              .service(web::resource("/path1").to(|| HttpResponse::Ok()))
              .service(web::resource("/path2").to(|| HttpResponse::Ok()))
              .service(web::resource("/path3").to(|| HttpResponse::MethodNotAllowed()))
      );
  ```

* `.with()`, `.with_async()` registration methods have been renamed to `.to()` and `.to_async()`.

  instead of

  ```rust
    App.new().resource("/welcome", |r| r.with(welcome))
  ```

  use `.to()` or `.to_async()` methods

  ```rust
    App.new().service(web::resource("/welcome").to(welcome))
  ```

* Passing arguments to handler with extractors, multiple arguments are allowed

  instead of

  ```rust
  fn welcome((body, req): (Bytes, HttpRequest)) -> ... {
    ...
  }
  ```

  use multiple arguments

  ```rust
  fn welcome(body: Bytes, req: HttpRequest) -> ... {
    ...
  }
  ```

* `.f()`, `.a()` and `.h()` handler registration methods have been removed.
  Use `.to()` for handlers and `.to_async()` for async handlers. Handler function
  must use extractors.

  instead of

  ```rust
    App.new().resource("/welcome", |r| r.f(welcome))
  ```

  use App's `to()` or `to_async()` methods

  ```rust
    App.new().service(web::resource("/welcome").to(welcome))
  ```

* `HttpRequest` does not provide access to request's payload stream.

  instead of

  ```rust
  fn index(req: &HttpRequest) -> Box<Future<Item=HttpResponse, Error=Error>> {
    req
       .payload()
       .from_err()
       .fold((), |_, chunk| {
            ...
        })
       .map(|_| HttpResponse::Ok().finish())
       .responder()
  }
  ```

  use `Payload` extractor

  ```rust
  fn index(stream: web::Payload) -> impl Future<Item=HttpResponse, Error=Error> {
     stream
       .from_err()
       .fold((), |_, chunk| {
            ...
        })
       .map(|_| HttpResponse::Ok().finish())
  }
  ```

* `State` is now `Data`.  You register Data during the App initialization process
  and then access it from handlers either using a Data extractor or using
  HttpRequest's api.

  instead of

  ```rust
    App.with_state(T)
  ```

  use App's `data` method

  ```rust
	App.new()
       .data(T)
  ```

  and either use the Data extractor within your handler

  ```rust
    use actix_web::web::Data;

	fn endpoint_handler(Data<T>)){
      ...
    }
  ```

  .. or access your Data element from the HttpRequest

  ```rust
	fn endpoint_handler(req: HttpRequest) {
		let data: Option<Data<T>> = req.app_data::<T>();
    }
  ```


* AsyncResponder is removed, use `.to_async()` registration method and `impl Future<>` as result type.

  instead of

  ```rust
	use actix_web::AsyncResponder;

    fn endpoint_handler(...) -> impl Future<Item=HttpResponse, Error=Error>{
		...
        .responder()
	}
  ```

  .. simply omit AsyncResponder and the corresponding responder() finish method


* Middleware

  instead of

  ```rust
      let app = App::new()
           .middleware(middleware::Logger::default())
  ```

  use `.wrap()` method

  ```rust
      let app = App::new()
           .wrap(middleware::Logger::default())
           .route("/index.html", web::get().to(index));
  ```

* `HttpRequest::body()`, `HttpRequest::urlencoded()`, `HttpRequest::json()`, `HttpRequest::multipart()`
  method have been removed. Use `Bytes`, `String`, `Form`, `Json`, `Multipart` extractors instead.

  instead of

  ```rust
  fn index(req: &HttpRequest) -> Responder {
     req.body()
       .and_then(|body| {
          ...
       })
  }
  ```

  use

  ```rust
  fn index(body: Bytes) -> Responder {
     ...
  }
  ```

* `actix_web::server` module has been removed. To start http server use `actix_web::HttpServer` type

* StaticFiles and NamedFile have been moved to a separate crate.

  instead of `use actix_web::fs::StaticFile`

  use `use actix_files::Files`

  instead of `use actix_web::fs::Namedfile`

  use `use actix_files::NamedFile`

* Multipart has been moved to a separate crate.

  instead of `use actix_web::multipart::Multipart`

  use `use actix_multipart::Multipart`

* Response compression is not enabled by default.
  To enable, use `Compress` middleware, `App::new().wrap(Compress::default())`.

* Session middleware moved to actix-session crate

* Actors support have been moved to `actix-web-actors` crate

* Custom Error

  Instead of error_response method alone, ResponseError now provides two methods: error_response and render_response respectively. Where, error_response creates the error response and render_response returns the error response to the caller. 

  Simplest migration from 0.7 to 1.0 shall include below method to the custom implementation of ResponseError:

  ```rust
  fn render_response(&self) -> HttpResponse {
    self.error_response()
  }
  ```

## 0.7.15

* The `' '` character is not percent decoded anymore before matching routes. If you need to use it in
  your routes, you should use `%20`.

  instead of

    ```rust
    fn main() {
         let app = App::new().resource("/my index", |r| {
             r.method(http::Method::GET)
                    .with(index);
         });
    }
    ```

  use

    ```rust
    fn main() {
         let app = App::new().resource("/my%20index", |r| {
             r.method(http::Method::GET)
                    .with(index);
         });
    }
    ```

* If you used `AsyncResult::async` you need to replace it with `AsyncResult::future`


## 0.7.4

* `Route::with_config()`/`Route::with_async_config()` always passes configuration objects as tuple
  even for handler with one parameter.


## 0.7

* `HttpRequest` does not implement `Stream` anymore. If you need to read request payload
  use `HttpMessage::payload()` method.

  instead of

    ```rust
    fn index(req: HttpRequest) -> impl Responder {
         req
            .from_err()
            .fold(...)
            ....
    }
    ```

  use `.payload()`

    ```rust
    fn index(req: HttpRequest) -> impl Responder {
         req
            .payload()  // <- get request payload stream
            .from_err()
            .fold(...)
            ....
    }
    ```

* [Middleware](https://actix.rs/actix-web/actix_web/middleware/trait.Middleware.html)
  trait uses `&HttpRequest` instead of `&mut HttpRequest`.

* Removed `Route::with2()` and `Route::with3()` use tuple of extractors instead.

    instead of

    ```rust
    fn index(query: Query<..>, info: Json<MyStruct) -> impl Responder {}
    ```

    use tuple of extractors and use `.with()` for registration:

    ```rust
    fn index((query, json): (Query<..>, Json<MyStruct)) -> impl Responder {}
    ```

* `Handler::handle()` uses `&self` instead of `&mut self`

* `Handler::handle()` accepts reference to `HttpRequest<_>` instead of value

* Removed deprecated `HttpServer::threads()`, use
  [HttpServer::workers()](https://actix.rs/actix-web/actix_web/server/struct.HttpServer.html#method.workers) instead.

* Renamed `client::ClientConnectorError::Connector` to
  `client::ClientConnectorError::Resolver`

* `Route::with()` does not return `ExtractorConfig`, to configure
  extractor use `Route::with_config()`

    instead of

    ```rust
    fn main() {
         let app = App::new().resource("/index.html", |r| {
             r.method(http::Method::GET)
                    .with(index)
                    .limit(4096);  // <- limit size of the payload
         });
    }
    ```

    use

    ```rust

    fn main() {
         let app = App::new().resource("/index.html", |r| {
             r.method(http::Method::GET)
                    .with_config(index, |cfg| { // <- register handler
                       cfg.limit(4096);  // <- limit size of the payload
                     })
         });
    }
    ```

* `Route::with_async()` does not return `ExtractorConfig`, to configure
  extractor use `Route::with_async_config()`


## 0.6

* `Path<T>` extractor return `ErrorNotFound` on failure instead of `ErrorBadRequest`

* `ws::Message::Close` now includes optional close reason.
  `ws::CloseCode::Status` and `ws::CloseCode::Empty` have been removed.

* `HttpServer::threads()` renamed to `HttpServer::workers()`.

* `HttpServer::start_ssl()` and `HttpServer::start_tls()` deprecated.
  Use `HttpServer::bind_ssl()` and `HttpServer::bind_tls()` instead.

* `HttpRequest::extensions()` returns read only reference to the request's Extension
  `HttpRequest::extensions_mut()` returns mutable reference.

* Instead of

   `use actix_web::middleware::{
        CookieSessionBackend, CookieSessionError, RequestSession,
        Session, SessionBackend, SessionImpl, SessionStorage};`

  use `actix_web::middleware::session`

   `use actix_web::middleware::session{CookieSessionBackend, CookieSessionError,
        RequestSession, Session, SessionBackend, SessionImpl, SessionStorage};`

* `FromRequest::from_request()` accepts mutable reference to a request

* `FromRequest::Result` has to implement `Into<Reply<Self>>`

* [`Responder::respond_to()`](
  https://actix.rs/actix-web/actix_web/trait.Responder.html#tymethod.respond_to)
  is generic over `S`

*  Use `Query` extractor instead of HttpRequest::query()`.

   ```rust
   fn index(q: Query<HashMap<String, String>>) -> Result<..> {
       ...
   }
   ```

   or

   ```rust
   let q = Query::<HashMap<String, String>>::extract(req);
   ```

* Websocket operations are implemented as `WsWriter` trait.
  you need to use `use actix_web::ws::WsWriter`


## 0.5

* `HttpResponseBuilder::body()`, `.finish()`, `.json()`
   methods return `HttpResponse` instead of `Result<HttpResponse>`

* `actix_web::Method`, `actix_web::StatusCode`, `actix_web::Version`
   moved to `actix_web::http` module

* `actix_web::header` moved to `actix_web::http::header`

* `NormalizePath` moved to `actix_web::http` module

* `HttpServer` moved to `actix_web::server`, added new `actix_web::server::new()` function,
  shortcut for `actix_web::server::HttpServer::new()`

* `DefaultHeaders` middleware does not use separate builder, all builder methods moved to type itself

* `StaticFiles::new()`'s show_index parameter removed, use `show_files_listing()` method instead.

* `CookieSessionBackendBuilder` removed, all methods moved to `CookieSessionBackend` type

* `actix_web::httpcodes` module is deprecated, `HttpResponse::Ok()`, `HttpResponse::Found()` and other `HttpResponse::XXX()`
   functions should be used instead

* `ClientRequestBuilder::body()` returns `Result<_, actix_web::Error>`
  instead of `Result<_, http::Error>`

* `Application` renamed to a `App`

* `actix_web::Reply`, `actix_web::Resource` moved to `actix_web::dev`
