# Changes

## Unreleased - 2021-xx-xx
### Changed
* Send headers within the redirect requests. [#2310]

[#2310]: https://github.com/actix/actix-web/pull/2310

## 3.0.0-beta.7 - 2021-06-26
### Changed
* Change compression algorithm features flags. [#2250]

[#2250]: https://github.com/actix/actix-web/pull/2250


## 3.0.0-beta.6 - 2021-06-17
* No significant changes since 3.0.0-beta.5.


## 3.0.0-beta.5 - 2021-04-17
### Removed
* Deprecated methods on `ClientRequest`: `if_true`, `if_some`. [#2148]

[#2148]: https://github.com/actix/actix-web/pull/2148


## 3.0.0-beta.4 - 2021-04-02
### Added
* Add `Client::headers` to get default mut reference of `HeaderMap` of client object. [#2114]

### Changed
* `ConnectorService` type is renamed to `BoxConnectorService`. [#2081]
* Fix http/https encoding when enabling `compress` feature. [#2116]
* Rename `TestResponse::header` to `append_header`, `set` to `insert_header`. `TestResponse` header
  methods now take `IntoHeaderPair` tuples. [#2094]

[#2081]: https://github.com/actix/actix-web/pull/2081
[#2094]: https://github.com/actix/actix-web/pull/2094
[#2114]: https://github.com/actix/actix-web/pull/2114
[#2116]: https://github.com/actix/actix-web/pull/2116


## 3.0.0-beta.3 - 2021-03-08
### Added
* `ClientResponse::timeout` for set the timeout of collecting response body. [#1931]
* `ClientBuilder::local_address` for bind to a local ip address for this client. [#2024]

### Changed
* Feature `cookies` is now optional and enabled by default. [#1981]
* `ClientBuilder::connector` method would take `actix_http::client::Connector<T, U>` type. [#2008]
* Basic auth password now takes blank passwords as an empty string instead of Option. [#2050]

### Removed
* `ClientBuilder::default` function [#2008]

[#1931]: https://github.com/actix/actix-web/pull/1931
[#1981]: https://github.com/actix/actix-web/pull/1981
[#2008]: https://github.com/actix/actix-web/pull/2008
[#2024]: https://github.com/actix/actix-web/pull/2024
[#2050]: https://github.com/actix/actix-web/pull/2050


## 3.0.0-beta.2 - 2021-02-10
### Added
* `ClientRequest::insert_header` method which allows using typed headers. [#1869]
* `ClientRequest::append_header` method which allows using typed headers. [#1869]
* `trust-dns` optional feature to enable `trust-dns-resolver` as client dns resolver. [#1969]

### Changed
* Relax default timeout for `Connector` to 5 seconds(original 1 second). [#1905]

### Removed
* `ClientRequest::set`; use `ClientRequest::insert_header`. [#1869]
* `ClientRequest::set_header`; use `ClientRequest::insert_header`. [#1869]
* `ClientRequest::set_header_if_none`; use `ClientRequest::insert_header_if_none`. [#1869]
* `ClientRequest::header`; use `ClientRequest::append_header`. [#1869]

[#1869]: https://github.com/actix/actix-web/pull/1869
[#1905]: https://github.com/actix/actix-web/pull/1905
[#1969]: https://github.com/actix/actix-web/pull/1969


## 3.0.0-beta.1 - 2021-01-07
### Changed
* Update `rand` to `0.8`
* Update `bytes` to `1.0`. [#1813]
* Update `rust-tls` to `0.19`. [#1813]

[#1813]: https://github.com/actix/actix-web/pull/1813


## 2.0.3 - 2020-11-29
### Fixed
* Ensure `actix-http` dependency uses same `serde_urlencoded`.


## 2.0.2 - 2020-11-25
### Changed
* Upgrade `serde_urlencoded` to `0.7`. [#1773]

[#1773]: https://github.com/actix/actix-web/pull/1773


## 2.0.1 - 2020-10-30
### Changed
* Upgrade `base64` to `0.13`. [#1744]
* Deprecate `ClientRequest::{if_some, if_true}`. [#1760]

### Fixed
* Use `Accept-Encoding: identity` instead of `Accept-Encoding: br` when no compression feature
  is enabled [#1737]

[#1737]: https://github.com/actix/actix-web/pull/1737
[#1760]: https://github.com/actix/actix-web/pull/1760
[#1744]: https://github.com/actix/actix-web/pull/1744


## 2.0.0 - 2020-09-11
### Changed
* `Client::build` was renamed to `Client::builder`.


## 2.0.0-beta.4 - 2020-09-09
### Changed
* Update actix-codec & actix-tls dependencies.


## 2.0.0-beta.3 - 2020-08-17
### Changed
* Update `rustls` to 0.18


## 2.0.0-beta.2 - 2020-07-21
### Changed
* Update `actix-http` dependency to 2.0.0-beta.2


## [2.0.0-beta.1] - 2020-07-14
### Changed
* Update `actix-http` dependency to 2.0.0-beta.1

## [2.0.0-alpha.2] - 2020-05-21

### Changed

* Implement `std::error::Error` for our custom errors [#1422]
* Bump minimum supported Rust version to 1.40
* Update `base64` dependency to 0.12

[#1422]: https://github.com/actix/actix-web/pull/1422

## [2.0.0-alpha.1] - 2020-03-11

* Update `actix-http` dependency to 2.0.0-alpha.2
* Update `rustls` dependency to 0.17
* ClientBuilder accepts initial_window_size and initial_connection_window_size HTTP2 configuration
* ClientBuilder allowing to set max_http_version to limit HTTP version to be used

## [1.0.1] - 2019-12-15

* Fix compilation with default features off

## [1.0.0] - 2019-12-13

* Release

## [1.0.0-alpha.3]

* Migrate to `std::future`


## [0.2.8] - 2019-11-06

* Add support for setting query from Serialize type for client request.


## [0.2.7] - 2019-09-25

### Added

* Remaining getter methods for `ClientRequest`'s private `head` field #1101


## [0.2.6] - 2019-09-12

### Added

* Export frozen request related types.


## [0.2.5] - 2019-09-11

### Added

* Add `FrozenClientRequest` to support retries for sending HTTP requests

### Changed

* Ensure that the `Host` header is set when initiating a WebSocket client connection.


## [0.2.4] - 2019-08-13

### Changed

* Update percent-encoding to "2.1"

* Update serde_urlencoded to "0.6.1"


## [0.2.3] - 2019-08-01

### Added

* Add `rustls` support


## [0.2.2] - 2019-07-01

### Changed

* Always append a colon after username in basic auth

* Upgrade `rand` dependency version to 0.7


## [0.2.1] - 2019-06-05

### Added

* Add license files

## [0.2.0] - 2019-05-12

### Added

* Allow to send headers in `Camel-Case` form.

### Changed

* Upgrade actix-http dependency.


## [0.1.1] - 2019-04-19

### Added

* Allow to specify server address for http and ws requests.

### Changed

* `ClientRequest::if_true()` and `ClientRequest::if_some()` use instance instead of ref


## [0.1.0] - 2019-04-16

* No changes


## [0.1.0-alpha.6] - 2019-04-14

### Changed

* Do not set default headers for websocket request


## [0.1.0-alpha.5] - 2019-04-12

### Changed

* Do not set any default headers

### Added

* Add Debug impl for BoxedSocket


## [0.1.0-alpha.4] - 2019-04-08

### Changed

* Update actix-http dependency


## [0.1.0-alpha.3] - 2019-04-02

### Added

* Export `MessageBody` type

* `ClientResponse::json()` - Loads and parse `application/json` encoded body


### Changed

* `ClientRequest::json()` accepts reference instead of object.

* `ClientResponse::body()` does not consume response object.

* Renamed `ClientRequest::close_connection()` to `ClientRequest::force_close()`


## [0.1.0-alpha.2] - 2019-03-29

### Added

* Per request and session wide request timeout.

* Session wide headers.

* Session wide basic and bearer auth.

* Re-export `actix_http::client::Connector`.


### Changed

* Allow to override request's uri

* Export `ws` sub-module with websockets related types


## [0.1.0-alpha.1] - 2019-03-28

* Initial impl
