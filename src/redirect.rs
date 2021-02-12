//! See [`Redirect`] for service/responder documentation.

use std::{borrow::Cow, ops::Deref};

use actix_service::fn_service;
use futures_util::future::ready;

use crate::{
    dev::{AppService, HttpServiceFactory, ResourceDef, ServiceRequest},
    http::{header, StatusCode},
    HttpRequest, HttpResponse, Responder,
};

/// An HTTP service for redirecting one path to another path or URL.
///
/// Redirects are either [relative](Redirect::to) or [absolute](Redirect::to).
///
/// By default, the "308 Temporary Redirect" status is used when responding.
/// See [this MDN article](mdn-redirects) on why 308 is preferred over 301.
///
/// # Examples
/// ```
/// use actix_web::{web, App, Redirect};
///
/// App::new()
///     // redirect "/duck" to DuckDuckGo
///     .service(Redirect::new("/duck", "https://duckduckgo.com/"))
///     .service(
///         // redirect "/api/old" to "/api/new" using `web::redirect` helper
///         web::scope("/api").service(web::redirect("/old", "/new"))
///     );
/// ```
///
/// [mdn-redirects]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Redirections#temporary_redirections
#[derive(Debug, Clone)]
pub struct Redirect {
    from: Cow<'static, str>,
    to: Cow<'static, str>,
    status_code: StatusCode,
}

impl Redirect {
    /// Create a new `Redirect` service, first providing the path that should be redirected.
    ///
    /// The default "to" location is the root path (`/`). It is expected that you should call either
    /// [`to`](Redirect::to) or [`to`](Redirect::to) afterwards.
    ///
    /// Note this function has no effect when used as a Responder
    ///
    /// Redirect to an address or path.
    ///
    /// Whatever argument is provided shall be used as-is when setting the redirect location.
    /// You can also use relative paths to navigate relative to the matched path.
    ///
    /// # Examples
    /// ```
    /// # use actix_web::Redirect;
    /// // redirects "/oh/hi/mark" to "/oh/bye/mark"
    /// Redirect::new("/oh/hi/mark", "../../bye/mark");
    /// ```
    pub fn new(
        from: impl Into<Cow<'static, str>>,
        to: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            status_code: StatusCode::PERMANENT_REDIRECT,
        }
    }

    /// Shortcut for creating a redirect to use as a Responder.
    ///
    /// Only receives a `to` argument since responders do not need to do route matching.
    pub fn to(to: impl Into<Cow<'static, str>>) -> Self {
        Self {
            from: "/".into(),
            to: to.into(),
            status_code: StatusCode::PERMANENT_REDIRECT,
        }
    }

    /// Use the "307 Temporary Redirect" status when responding.
    ///
    /// See [this MDN article](mdn-redirects) on why 307 is preferred over 302.
    ///
    /// [mdn-redirects]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Redirections#temporary_redirections
    pub fn temporary(self) -> Self {
        self.using_status_code(StatusCode::TEMPORARY_REDIRECT)
    }

    /// Allows the use of custom status codes for less common redirect types.
    ///
    /// In most cases, the default status ("308 Permanent Redirect") or using the `temporary`
    /// method, which uses the "307 Temporary Redirect" status have more consistent behavior than
    /// 301 and 302 codes, respectively.
    ///
    /// ```
    /// # use actix_web::{http::StatusCode, Redirect};
    /// // redirects would use "301 Moved Permanently" status code
    /// Redirect::new("/old", "/new")
    ///     .using_status_code(StatusCode::MOVED_PERMANENTLY);
    ///
    /// // redirects would use "302 Found" status code
    /// Redirect::new("/old", "/new")
    ///     .using_status_code(StatusCode::FOUND);
    /// ```
    pub fn using_status_code(mut self, status: StatusCode) -> Self {
        self.status_code = status;
        self
    }
}

impl HttpServiceFactory for Redirect {
    fn register(self, config: &mut AppService) {
        let Self {
            from,
            to,
            status_code,
        } = self;

        let rdef = ResourceDef::new(from.into_owned());
        let redirect_factory = fn_service(move |req: ServiceRequest| {
            ready(Ok(req.into_response(
                HttpResponse::build(status_code)
                    .insert_header((header::LOCATION, to.deref()))
                    .finish(),
            )))
        });

        config.register_service(rdef, None, redirect_factory, None)
    }
}

impl Responder for Redirect {
    fn respond_to(self, _req: &HttpRequest) -> HttpResponse {
        let Redirect {
            to, status_code, ..
        } = self;

        HttpResponse::build(status_code)
            .insert_header((header::LOCATION, to.into_owned()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{dev::Service, http::StatusCode, test, App};

    #[actix_rt::test]
    async fn absolute_redirects() {
        let redirector = Redirect::new("/one", "/two");

        let svc = test::init_service(App::new().service(redirector)).await;

        let req = test::TestRequest::default().uri("/one").to_request();
        let res = svc.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(308).unwrap());
        let hdr = res.headers().get(&header::LOCATION).unwrap();
        assert_eq!(hdr.to_str().unwrap(), "/two");
    }

    #[actix_rt::test]
    async fn relative_redirects() {
        let redirector = Redirect::new("/one", "two");

        let svc = test::init_service(App::new().service(redirector)).await;

        let req = test::TestRequest::default().uri("/one").to_request();
        let res = svc.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(308).unwrap());
        let hdr = res.headers().get(&header::LOCATION).unwrap();
        assert_eq!(hdr.to_str().unwrap(), "two");
    }

    #[actix_rt::test]
    async fn temporary_redirects() {
        let external_service =
            Redirect::new("/external", "https://duck.com").temporary();

        let svc = test::init_service(App::new().service(external_service)).await;

        let req = test::TestRequest::default().uri("/external").to_request();
        let res = svc.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(307).unwrap());
        let hdr = res.headers().get(&header::LOCATION).unwrap();
        assert_eq!(hdr.to_str().unwrap(), "https://duck.com");
    }

    #[actix_rt::test]
    async fn as_responder() {
        let responder = Redirect::to("https://duck.com").temporary();

        let req = test::TestRequest::default().to_http_request();
        let res = responder.respond_to(&req);

        assert_eq!(res.status(), StatusCode::from_u16(307).unwrap());
        let hdr = res.headers().get(&header::LOCATION).unwrap();
        assert_eq!(hdr.to_str().unwrap(), "https://duck.com");
    }
}
