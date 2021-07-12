# Changes

## Unreleased - 2021-xx-xx
* Prevent directory traversal attack by checking if the request path is a descendant of the
  `serve_from` directory. E.g. If the files are served from `./test/`, and someone requests
  `..%5c/some-secret-file`, they will get a 403 Forbidden response. [#2059]

[#2059]: https://github.com/actix/actix-web/pull/2059


## 0.6.0-beta.6 - 2021-06-26
* Added `Files::path_filter()`. [#2274]
* `Files::show_files_listing()` can now be used with `Files::index_file()` to show files listing as a fallback when the index file is not found. [#2228]

[#2274]: https://github.com/actix/actix-web/pull/2274
[#2228]: https://github.com/actix/actix-web/pull/2228


## 0.6.0-beta.5 - 2021-06-17
* `NamedFile` now implements `ServiceFactory` and `HttpServiceFactory` making it much more useful in routing. For example, it can be used directly as a default service. [#2135]
* For symbolic links, `Content-Disposition` header no longer shows the filename of the original file. [#2156]
* `Files::redirect_to_slash_directory()` now works as expected when used with `Files::show_files_listing()`. [#2225]
* `application/{javascript, json, wasm}` mime type now have `inline` disposition by default. [#2257]

[#2135]: https://github.com/actix/actix-web/pull/2135
[#2156]: https://github.com/actix/actix-web/pull/2156
[#2225]: https://github.com/actix/actix-web/pull/2225
[#2257]: https://github.com/actix/actix-web/pull/2257


## 0.6.0-beta.4 - 2021-04-02
* Add support for `.guard` in `Files` to selectively filter `Files` services. [#2046]

[#2046]: https://github.com/actix/actix-web/pull/2046


## 0.6.0-beta.3 - 2021-03-09
* No notable changes.


## 0.6.0-beta.2 - 2021-02-10
* Fix If-Modified-Since and If-Unmodified-Since to not compare using sub-second timestamps. [#1887]
* Replace `v_htmlescape` with `askama_escape`. [#1953]

[#1887]: https://github.com/actix/actix-web/pull/1887
[#1953]: https://github.com/actix/actix-web/pull/1953


## 0.6.0-beta.1 - 2021-01-07
* `HttpRange::parse` now has its own error type.
* Update `bytes` to `1.0`. [#1813]

[#1813]: https://github.com/actix/actix-web/pull/1813


## 0.5.0 - 2020-12-26
* Optionally support hidden files/directories. [#1811]

[#1811]: https://github.com/actix/actix-web/pull/1811


## 0.4.1 - 2020-11-24
* Clarify order of parameters in `Files::new` and improve docs.


## 0.4.0 - 2020-10-06
* Add `Files::prefer_utf8` option that adds UTF-8 charset on certain response types. [#1714]

[#1714]: https://github.com/actix/actix-web/pull/1714


## 0.3.0 - 2020-09-11
* No significant changes from 0.3.0-beta.1.


## 0.3.0-beta.1 - 2020-07-15
* Update `v_htmlescape` to 0.10
* Update `actix-web` and `actix-http` dependencies to beta.1


## 0.3.0-alpha.1 - 2020-05-23
* Update `actix-web` and `actix-http` dependencies to alpha
* Fix some typos in the docs
* Bump minimum supported Rust version to 1.40
* Support sending Content-Length when Content-Range is specified [#1384]

[#1384]: https://github.com/actix/actix-web/pull/1384


## 0.2.1 - 2019-12-22
* Use the same format for file URLs regardless of platforms


## 0.2.0 - 2019-12-20
* Fix BodyEncoding trait import #1220


## 0.2.0-alpha.1 - 2019-12-07
* Migrate to `std::future`


## 0.1.7 - 2019-11-06
* Add an additional `filename*` param in the `Content-Disposition` header of
  `actix_files::NamedFile` to be more compatible. (#1151)

## 0.1.6 - 2019-10-14
* Add option to redirect to a slash-ended path `Files` #1132


## 0.1.5 - 2019-10-08
* Bump up `mime_guess` crate version to 2.0.1
* Bump up `percent-encoding` crate version to 2.1
* Allow user defined request guards for `Files` #1113


## 0.1.4 - 2019-07-20
* Allow to disable `Content-Disposition` header #686


## 0.1.3 - 2019-06-28
* Do not set `Content-Length` header, let actix-http set it #930


## 0.1.2 - 2019-06-13
* Content-Length is 0 for NamedFile HEAD request #914
* Fix ring dependency from actix-web default features for #741


## 0.1.1 - 2019-06-01
* Static files are incorrectly served as both chunked and with length #812


## 0.1.0 - 2019-05-25
* NamedFile last-modified check always fails due to nano-seconds in file modified date #820


## 0.1.0-beta.4 - 2019-05-12
* Update actix-web to beta.4


## 0.1.0-beta.1 - 2019-04-20
* Update actix-web to beta.1


## 0.1.0-alpha.6 - 2019-04-14
* Update actix-web to alpha6


## 0.1.0-alpha.4 - 2019-04-08
* Update actix-web to alpha4


## 0.1.0-alpha.2 - 2019-04-02
* Add default handler support


## 0.1.0-alpha.1 - 2019-03-28
* Initial impl
