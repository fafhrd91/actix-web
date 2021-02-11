# Changes

## Unreleased - 2021-xx-xx
* Add `ws:WsResponseBuilder` for building web socket session response. [#1920]

[#1920]: https://github.com/actix/actix-web/pull/1920


## 4.0.0-beta.2 - 2021-02-10
* No notable changes.


## 4.0.0-beta.1 - 2021-01-07
* Update `pin-project` to `1.0`.
* Update `bytes` to `1.0`. [#1813]
* `WebsocketContext::text` now takes an `Into<bytestring::ByteString>`. [#1864]

[#1813]: https://github.com/actix/actix-web/pull/1813
[#1864]: https://github.com/actix/actix-web/pull/1864


## 3.0.0 - 2020-09-11
* No significant changes from `3.0.0-beta.2`.


## 3.0.0-beta.2 - 2020-09-10
* Update `actix-*` dependencies to latest versions.


## [3.0.0-beta.1] - 2020-xx-xx
* Update `actix-web` & `actix-http` dependencies to beta.1
* Bump minimum supported Rust version to 1.40


## [3.0.0-alpha.1] - 2020-05-08
* Update the actix-web dependency to 3.0.0-alpha.1
* Update the actix dependency to 0.10.0-alpha.2
* Update the actix-http dependency to 2.0.0-alpha.3

## [2.0.0] - 2019-12-20

* Release

## [2.0.0-alpha.1] - 2019-12-15

* Migrate to actix-web 2.0.0

## [1.0.4] - 2019-12-07

* Allow comma-separated websocket subprotocols without spaces (#1172)

## [1.0.3] - 2019-11-14

* Update actix-web and actix-http dependencies

## [1.0.2] - 2019-07-20

* Add `ws::start_with_addr()`, returning the address of the created actor, along
  with the `HttpResponse`.

* Add support for specifying protocols on websocket handshake #835

## [1.0.1] - 2019-06-28

* Allow to use custom ws codec with `WebsocketContext` #925

## [1.0.0] - 2019-05-29

* Update actix-http and actix-web

## [0.1.0-alpha.3] - 2019-04-02

* Update actix-http and actix-web

## [0.1.0-alpha.2] - 2019-03-29

* Update actix-http and actix-web

## [0.1.0-alpha.1] - 2019-03-28

* Initial impl
