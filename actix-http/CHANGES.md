# Changes

## Unreleased - 2021-xx-xx
### Added
* `CloneableExtensions` object for use in `on_connect` handlers. [#2327]

### Changed
* `on_connect_ext` methods now receive a `CloneableExtensions` object. [#2327]

[#2327]: https://github.com/actix/actix-web/pull/2327


## 3.0.0-beta.8 - 2021-06-26
### Changed
* Change compression algorithm features flags. [#2250]

### Removed
* `downcast` and `downcast_get_type_id` macros. [#2291]

[#2291]: https://github.com/actix/actix-web/pull/2291
[#2250]: https://github.com/actix/actix-web/pull/2250


## 3.0.0-beta.7 - 2021-06-17
### Added
* Alias `body::Body` as `body::AnyBody`. [#2215]
* `BoxAnyBody`: a boxed message body with boxed errors. [#2183]
* Re-export `http` crate's `Error` type as `error::HttpError`. [#2171]
* Re-export `StatusCode`, `Method`, `Version` and `Uri` at the crate root. [#2171]
* Re-export `ContentEncoding` and `ConnectionType` at the crate root. [#2171]
* `Response::into_body` that consumes response and returns body type. [#2201]
* `impl Default` for `Response`. [#2201]
* Add zstd support for `ContentEncoding`. [#2244]

### Changed
* The `MessageBody` trait now has an associated `Error` type. [#2183]
* All error trait bounds in server service builders have changed from `Into<Error>` to `Into<Response<AnyBody>>`. [#2253]
* All error trait bounds in message body and stream impls changed from `Into<Error>` to `Into<Box<dyn std::error::Error>>`. [#2253]
* Places in `Response` where `ResponseBody<B>` was received or returned now simply use `B`. [#2201]
* `header` mod is now public. [#2171]
* `uri` mod is now public. [#2171]
* Update `language-tags` to `0.3`.
* Reduce the level from `error` to `debug` for the log line that is emitted when a `500 Internal Server Error` is built using `HttpResponse::from_error`. [#2201]
* `ResponseBuilder::message_body` now returns a `Result`. [#2201]
* Remove `Unpin` bound on `ResponseBuilder::streaming`. [#2253]
* `HttpServer::{listen_rustls(), bind_rustls()}` now honor the ALPN protocols in the configuation parameter. [#2226]

### Removed
* Stop re-exporting `http` crate's `HeaderMap` types in addition to ours. [#2171]
* Down-casting for `MessageBody` types. [#2183]
* `error::Result` alias. [#2201]
* Error field from `Response` and `Response::error`. [#2205]
* `impl Future` for `Response`. [#2201]
* `Response::take_body` and old `Response::into_body` method that casted body type. [#2201]
* `InternalError` and all the error types it constructed. [#2215]
* Conversion (`impl Into`) of `Response<Body>` and `ResponseBuilder` to `Error`. [#2215]

[#2171]: https://github.com/actix/actix-web/pull/2171
[#2183]: https://github.com/actix/actix-web/pull/2183
[#2196]: https://github.com/actix/actix-web/pull/2196
[#2201]: https://github.com/actix/actix-web/pull/2201
[#2205]: https://github.com/actix/actix-web/pull/2205
[#2215]: https://github.com/actix/actix-web/pull/2215
[#2253]: https://github.com/actix/actix-web/pull/2253
[#2244]: https://github.com/actix/actix-web/pull/2244



## 3.0.0-beta.6 - 2021-04-17
### Added
* `impl<T: MessageBody> MessageBody for Pin<Box<T>>`. [#2152]
* `Response::{ok, bad_request, not_found, internal_server_error}`. [#2159]
* Helper `body::to_bytes` for async collecting message body into Bytes. [#2158]

### Changes
* The type parameter of `Response` no longer has a default. [#2152]
* The `Message` variant of `body::Body` is now `Pin<Box<dyn MessageBody>>`. [#2152]
* `BodyStream` and `SizedStream` are no longer restricted to Unpin types. [#2152]
* Error enum types are marked `#[non_exhaustive]`. [#2161]

### Removed
* `cookies` feature flag. [#2065]
* Top-level `cookies` mod (re-export). [#2065]
* `HttpMessage` trait loses the `cookies` and `cookie` methods. [#2065]
* `impl ResponseError for CookieParseError`. [#2065]
* Deprecated methods on `ResponseBuilder`: `if_true`, `if_some`. [#2148]
* `ResponseBuilder::json`. [#2148]
* `ResponseBuilder::{set_header, header}`. [#2148]
* `impl From<serde_json::Value> for Body`. [#2148]
* `Response::build_from`. [#2159]
* Most of the status code builders on `Response`. [#2159]

[#2065]: https://github.com/actix/actix-web/pull/2065
[#2148]: https://github.com/actix/actix-web/pull/2148
[#2152]: https://github.com/actix/actix-web/pull/2152
[#2159]: https://github.com/actix/actix-web/pull/2159
[#2158]: https://github.com/actix/actix-web/pull/2158
[#2161]: https://github.com/actix/actix-web/pull/2161


## 3.0.0-beta.5 - 2021-04-02
### Added
* `client::Connector::handshake_timeout` method for customizing TLS connection handshake timeout. [#2081]
* `client::ConnectorService` as `client::Connector::finish` method's return type [#2081]
* `client::ConnectionIo` trait alias [#2081]

### Changed
* `client::Connector` type now only have one generic type for `actix_service::Service`. [#2063]

### Removed
* Common typed HTTP headers were moved to actix-web. [2094]
* `ResponseError` impl for `actix_utils::timeout::TimeoutError`. [#2127]

[#2063]: https://github.com/actix/actix-web/pull/2063
[#2081]: https://github.com/actix/actix-web/pull/2081
[#2094]: https://github.com/actix/actix-web/pull/2094
[#2127]: https://github.com/actix/actix-web/pull/2127


## 3.0.0-beta.4 - 2021-03-08
### Changed
* Feature `cookies` is now optional and disabled by default. [#1981]
* `ws::hash_key` now returns array. [#2035]
* `ResponseBuilder::json` now takes `impl Serialize`. [#2052]

### Removed
* Re-export of `futures_channel::oneshot::Canceled` is removed from `error` mod. [#1994]
* `ResponseError` impl for `futures_channel::oneshot::Canceled` is removed. [#1994]

[#1981]: https://github.com/actix/actix-web/pull/1981
[#1994]: https://github.com/actix/actix-web/pull/1994
[#2035]: https://github.com/actix/actix-web/pull/2035
[#2052]: https://github.com/actix/actix-web/pull/2052


## 3.0.0-beta.3 - 2021-02-10
* No notable changes.


## 3.0.0-beta.2 - 2021-02-10
### Added
* `IntoHeaderPair` trait that allows using typed and untyped headers in the same methods. [#1869]
* `ResponseBuilder::insert_header` method which allows using typed headers. [#1869]
* `ResponseBuilder::append_header` method which allows using typed headers. [#1869]
* `TestRequest::insert_header` method which allows using typed headers. [#1869]
* `ContentEncoding` implements all necessary header traits. [#1912]
* `HeaderMap::len_keys` has the behavior of the old `len` method. [#1964]
* `HeaderMap::drain` as an efficient draining iterator. [#1964]
* Implement `IntoIterator` for owned `HeaderMap`. [#1964]
* `trust-dns` optional feature to enable `trust-dns-resolver` as client dns resolver. [#1969]

### Changed
* `ResponseBuilder::content_type` now takes an `impl IntoHeaderValue` to support using typed
  `mime` types. [#1894]
* Renamed `IntoHeaderValue::{try_into => try_into_value}` to avoid ambiguity with std
  `TryInto` trait. [#1894]
* `Extensions::insert` returns Option of replaced item. [#1904]
* Remove `HttpResponseBuilder::json2()`. [#1903]
* Enable `HttpResponseBuilder::json()` to receive data by value and reference. [#1903]
* `client::error::ConnectError` Resolver variant contains `Box<dyn std::error::Error>` type. [#1905]
* `client::ConnectorConfig` default timeout changed to 5 seconds. [#1905]
* Simplify `BlockingError` type to a unit struct. It's now only triggered when blocking thread pool
  is dead. [#1957]
* `HeaderMap::len` now returns number of values instead of number of keys. [#1964]
* `HeaderMap::insert` now returns iterator of removed values. [#1964]
* `HeaderMap::remove` now returns iterator of removed values. [#1964]

### Removed
* `ResponseBuilder::set`; use `ResponseBuilder::insert_header`. [#1869]
* `ResponseBuilder::set_header`; use `ResponseBuilder::insert_header`. [#1869]
* `ResponseBuilder::header`; use `ResponseBuilder::append_header`. [#1869]
* `TestRequest::with_hdr`; use `TestRequest::default().insert_header()`. [#1869]
* `TestRequest::with_header`; use `TestRequest::default().insert_header()`. [#1869]
* `actors` optional feature. [#1969]
* `ResponseError` impl for `actix::MailboxError`. [#1969]

### Documentation
* Vastly improve docs and add examples for `HeaderMap`. [#1964]

[#1869]: https://github.com/actix/actix-web/pull/1869
[#1894]: https://github.com/actix/actix-web/pull/1894
[#1903]: https://github.com/actix/actix-web/pull/1903
[#1904]: https://github.com/actix/actix-web/pull/1904
[#1905]: https://github.com/actix/actix-web/pull/1905
[#1912]: https://github.com/actix/actix-web/pull/1912
[#1957]: https://github.com/actix/actix-web/pull/1957
[#1964]: https://github.com/actix/actix-web/pull/1964
[#1969]: https://github.com/actix/actix-web/pull/1969


## 3.0.0-beta.1 - 2021-01-07
### Added
* Add `Http3` to `Protocol` enum for future compatibility and also mark `#[non_exhaustive]`.

### Changed
* Update `actix-*` dependencies to tokio `1.0` based versions. [#1813]
* Bumped `rand` to `0.8`.
* Update `bytes` to `1.0`. [#1813]
* Update `h2` to `0.3`. [#1813]
* The `ws::Message::Text` enum variant now contains a `bytestring::ByteString`. [#1864]

### Removed
* Deprecated `on_connect` methods have been removed. Prefer the new
  `on_connect_ext` technique. [#1857]
* Remove `ResponseError` impl for `actix::actors::resolver::ResolverError`
  due to deprecate of resolver actor. [#1813]
* Remove `ConnectError::SslHandshakeError` and re-export of `HandshakeError`.
  due to the removal of this type from `tokio-openssl` crate. openssl handshake 
  error would return as `ConnectError::SslError`. [#1813]
* Remove `actix-threadpool` dependency. Use `actix_rt::task::spawn_blocking`.
  Due to this change `actix_threadpool::BlockingError` type is moved into 
  `actix_http::error` module. [#1878]

[#1813]: https://github.com/actix/actix-web/pull/1813
[#1857]: https://github.com/actix/actix-web/pull/1857
[#1864]: https://github.com/actix/actix-web/pull/1864
[#1878]: https://github.com/actix/actix-web/pull/1878


## 2.2.0 - 2020-11-25
### Added
* HttpResponse builders for 1xx status codes. [#1768]
* `Accept::mime_precedence` and `Accept::mime_preference`. [#1793]
* `TryFrom<u16>` and `TryFrom<f32>` for `http::header::Quality`. [#1797]

### Fixed
* Started dropping `transfer-encoding: chunked` and `Content-Length` for 1XX and 204 responses. [#1767]

### Changed
* Upgrade `serde_urlencoded` to `0.7`. [#1773]

[#1773]: https://github.com/actix/actix-web/pull/1773
[#1767]: https://github.com/actix/actix-web/pull/1767
[#1768]: https://github.com/actix/actix-web/pull/1768
[#1793]: https://github.com/actix/actix-web/pull/1793
[#1797]: https://github.com/actix/actix-web/pull/1797


## 2.1.0 - 2020-10-30
### Added
* Added more flexible `on_connect_ext` methods for on-connect handling. [#1754]

### Changed
* Upgrade `base64` to `0.13`. [#1744]
* Upgrade `pin-project` to `1.0`. [#1733]
* Deprecate `ResponseBuilder::{if_some, if_true}`. [#1760]

[#1760]: https://github.com/actix/actix-web/pull/1760
[#1754]: https://github.com/actix/actix-web/pull/1754
[#1733]: https://github.com/actix/actix-web/pull/1733
[#1744]: https://github.com/actix/actix-web/pull/1744


## 2.0.0 - 2020-09-11
* No significant changes from `2.0.0-beta.4`.


## 2.0.0-beta.4 - 2020-09-09
### Changed
* Update actix-codec and actix-utils dependencies.
* Update actix-connect and actix-tls dependencies.


## 2.0.0-beta.3 - 2020-08-14
### Fixed
* Memory leak of `client::pool::ConnectorPoolSupport`. [#1626]

[#1626]: https://github.com/actix/actix-web/pull/1626


## 2.0.0-beta.2 - 2020-07-21
### Fixed
* Potential UB in h1 decoder using uninitialized memory. [#1614]

### Changed
* Fix illegal chunked encoding. [#1615]

[#1614]: https://github.com/actix/actix-web/pull/1614
[#1615]: https://github.com/actix/actix-web/pull/1615


## 2.0.0-beta.1 - 2020-07-11
### Changed
* Migrate cookie handling to `cookie` crate. [#1558]
* Update `sha-1` to 0.9. [#1586]
* Fix leak in client pool. [#1580]
* MSRV is now 1.41.1.

[#1558]: https://github.com/actix/actix-web/pull/1558
[#1586]: https://github.com/actix/actix-web/pull/1586
[#1580]: https://github.com/actix/actix-web/pull/1580


## 2.0.0-alpha.4 - 2020-05-21
### Changed
* Bump minimum supported Rust version to 1.40
* content_length function is removed, and you can set Content-Length by calling
  no_chunking function [#1439]
* `BodySize::Sized64` variant has been removed. `BodySize::Sized` now receives a
  `u64` instead of a `usize`.
* Update `base64` dependency to 0.12

### Fixed
* Support parsing of `SameSite=None` [#1503]

[#1439]: https://github.com/actix/actix-web/pull/1439
[#1503]: https://github.com/actix/actix-web/pull/1503


## 2.0.0-alpha.3 - 2020-05-08
### Fixed
* Correct spelling of ConnectError::Unresolved [#1487]
* Fix a mistake in the encoding of websocket continuation messages wherein
  Item::FirstText and Item::FirstBinary are each encoded as the other.

### Changed
* Implement `std::error::Error` for our custom errors [#1422]
* Remove `failure` support for `ResponseError` since that crate
  will be deprecated in the near future.

[#1422]: https://github.com/actix/actix-web/pull/1422
[#1487]: https://github.com/actix/actix-web/pull/1487


## 2.0.0-alpha.2 - 2020-03-07
### Changed
* Update `actix-connect` and `actix-tls` dependency to 2.0.0-alpha.1. [#1395]
* Change default initial window size and connection window size for HTTP2 to 2MB and 1MB
  respectively to improve download speed for awc when downloading large objects. [#1394]
* client::Connector accepts initial_window_size and initial_connection_window_size
  HTTP2 configuration. [#1394]
* client::Connector allowing to set max_http_version to limit HTTP version to be used. [#1394]

[#1394]: https://github.com/actix/actix-web/pull/1394
[#1395]: https://github.com/actix/actix-web/pull/1395


## 2.0.0-alpha.1 - 2020-02-27
### Changed
* Update the `time` dependency to 0.2.7.
* Moved actors messages support from actix crate, enabled with feature `actors`.
* Breaking change: trait MessageBody requires Unpin and accepting `Pin<&mut Self>` instead of
  `&mut self` in the poll_next().
* MessageBody is not implemented for &'static [u8] anymore.

### Fixed
* Allow `SameSite=None` cookies to be sent in a response.


## 1.0.1 - 2019-12-20
### Fixed
* Poll upgrade service's readiness from HTTP service handlers
* Replace brotli with brotli2 #1224


## 1.0.0 - 2019-12-13
### Added
* Add websockets continuation frame support

### Changed
* Replace `flate2-xxx` features with `compress`


## 1.0.0-alpha.5 - 2019-12-09
### Fixed
* Check `Upgrade` service readiness before calling it
* Fix buffer remaining capacity calculation

### Changed
* Websockets: Ping and Pong should have binary data #1049


## 1.0.0-alpha.4 - 2019-12-08
### Added
* Add impl ResponseBuilder for Error

### Changed
* Use rust based brotli compression library

## 1.0.0-alpha.3 - 2019-12-07
### Changed
* Migrate to tokio 0.2
* Migrate to `std::future`


## 0.2.11 - 2019-11-06
### Added
* Add support for serde_json::Value to be passed as argument to ResponseBuilder.body()
* Add an additional `filename*` param in the `Content-Disposition` header of
  `actix_files::NamedFile` to be more compatible. (#1151)
* Allow to use `std::convert::Infallible` as `actix_http::error::Error`

### Fixed
* To be compatible with non-English error responses, `ResponseError` rendered with `text/plain;
  charset=utf-8` header [#1118]

[#1878]: https://github.com/actix/actix-web/pull/1878


## 0.2.10 - 2019-09-11
### Added
* Add support for sending HTTP requests with `Rc<RequestHead>` in addition to sending HTTP requests
  with `RequestHead`

### Fixed
* h2 will use error response #1080
* on_connect result isn't added to request extensions for http2 requests #1009


## 0.2.9 - 2019-08-13
### Changed
* Dropped the `byteorder`-dependency in favor of `stdlib`-implementation
* Update percent-encoding to 2.1
* Update serde_urlencoded to 0.6.1

### Fixed
* Fixed a panic in the HTTP2 handshake in client HTTP requests (#1031)


## 0.2.8 - 2019-08-01
### Added
* Add `rustls` support
* Add `Clone` impl for `HeaderMap`

### Fixed
* awc client panic #1016
* Invalid response with compression middleware enabled, but compression-related features
  disabled #997


## 0.2.7 - 2019-07-18
### Added
* Add support for downcasting response errors #986


## 0.2.6 - 2019-07-17
### Changed
* Replace `ClonableService` with local copy
* Upgrade `rand` dependency version to 0.7


## 0.2.5 - 2019-06-28
### Added
* Add `on-connect` callback, `HttpServiceBuilder::on_connect()` #946

### Changed
* Use `encoding_rs` crate instead of unmaintained `encoding` crate
* Add `Copy` and `Clone` impls for `ws::Codec`


## 0.2.4 - 2019-06-16
### Fixed
* Do not compress NoContent (204) responses #918


## 0.2.3 - 2019-06-02
### Added
* Debug impl for ResponseBuilder
* From SizedStream and BodyStream for Body

### Changed
* SizedStream uses u64


## 0.2.2 - 2019-05-29
### Fixed
* Parse incoming stream before closing stream on disconnect #868


## 0.2.1 - 2019-05-25
### Fixed
* Handle socket read disconnect


## 0.2.0 - 2019-05-12
### Changed
* Update actix-service to 0.4
* Expect and upgrade services accept `ServerConfig` config.

### Deleted
* `OneRequest` service


## 0.1.5 - 2019-05-04
### Fixed
* Clean up response extensions in response pool #817


## 0.1.4 - 2019-04-24
### Added
* Allow to render h1 request headers in `Camel-Case`

### Fixed
* Read until eof for http/1.0 responses #771


## 0.1.3 - 2019-04-23
### Fixed
* Fix http client pool management
* Fix http client wait queue management #794


## 0.1.2 - 2019-04-23
### Fixed
* Fix BorrowMutError panic in client connector #793


## 0.1.1 - 2019-04-19
### Changed
* Cookie::max_age() accepts value in seconds
* Cookie::max_age_time() accepts value in time::Duration
* Allow to specify server address for client connector


## 0.1.0 - 2019-04-16
### Added
* Expose peer addr via `Request::peer_addr()` and `RequestHead::peer_addr`

### Changed
* `actix_http::encoding` always available
* use trust-dns-resolver 0.11.0


## 0.1.0-alpha.5 - 2019-04-12
### Added
* Allow to use custom service for upgrade requests
* Added `h1::SendResponse` future.

### Changed
* MessageBody::length() renamed to MessageBody::size() for consistency
* ws handshake verification functions take RequestHead instead of Request


## 0.1.0-alpha.4 - 2019-04-08
### Added
* Allow to use custom `Expect` handler
* Add minimal `std::error::Error` impl for `Error`

### Changed
* Export IntoHeaderValue
* Render error and return as response body
* Use thread pool for response body compression

### Deleted
* Removed PayloadBuffer


## 0.1.0-alpha.3 - 2019-04-02
### Added
* Warn when an unsealed private cookie isn't valid UTF-8

### Fixed
* Rust 1.31.0 compatibility
* Preallocate read buffer for h1 codec
* Detect socket disconnection during protocol selection


## 0.1.0-alpha.2 - 2019-03-29
### Added
* Added ws::Message::Nop, no-op websockets message

### Changed
* Do not use thread pool for decompression if chunk size is smaller than 2048.


## 0.1.0-alpha.1 - 2019-03-28
* Initial impl
