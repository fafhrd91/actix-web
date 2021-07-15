//! Request extractors

use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use actix_http::http::{Method, Uri};
use actix_utils::future::{ok, Ready};
use futures_core::ready;
use futures_util::future::{maybe_done, MaybeDone};

use crate::{dev::Payload, Error, HttpRequest};

/// Trait implemented by types that can be extracted from request.
///
/// Types that implement this trait can be used with `Route` handlers.
pub trait FromRequest: Sized {
    /// Configuration for this extractor.
    type Config: Default + 'static;

    /// The associated error which can be returned.
    type Error: Into<Error>;

    /// Future that resolves to a Self.
    type Future: Future<Output = Result<Self, Self::Error>>;

    /// Create a Self from request parts asynchronously.
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future;

    /// Create a Self from request head asynchronously.
    ///
    /// This method is short for `T::from_request(req, &mut Payload::None)`.
    fn extract(req: &HttpRequest) -> Self::Future {
        Self::from_request(req, &mut Payload::None)
    }

    /// Create and configure config instance.
    fn configure<F>(f: F) -> Self::Config
    where
        F: FnOnce(Self::Config) -> Self::Config,
    {
        f(Self::Config::default())
    }
}

/// Optionally extract a field from the request
///
/// If the FromRequest for T fails, return None rather than returning an error response
///
/// # Examples
/// ```
/// use actix_web::{web, dev, App, Error, HttpRequest, FromRequest};
/// use actix_web::error::ErrorBadRequest;
/// use futures_util::future::{ok, err, Ready};
/// use serde::Deserialize;
/// use rand;
///
/// #[derive(Debug, Deserialize)]
/// struct Thing {
///     name: String
/// }
///
/// impl FromRequest for Thing {
///     type Error = Error;
///     type Future = Ready<Result<Self, Self::Error>>;
///     type Config = ();
///
///     fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
///         if rand::random() {
///             ok(Thing { name: "thingy".into() })
///         } else {
///             err(ErrorBadRequest("no luck"))
///         }
///
///     }
/// }
///
/// /// extract `Thing` from request
/// async fn index(supplied_thing: Option<Thing>) -> String {
///     match supplied_thing {
///         // Puns not intended
///         Some(thing) => format!("Got something: {:?}", thing),
///         None => format!("No thing!")
///     }
/// }
///
/// fn main() {
///     let app = App::new().service(
///         web::resource("/users/:first").route(
///             web::post().to(index))
///     );
/// }
/// ```
impl<T: 'static> FromRequest for Option<T>
where
    T: FromRequest,
    T::Future: 'static,
{
    type Error = Error;
    type Future = FromRequestOptFuture<T::Future>;
    type Config = T::Config;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        FromRequestOptFuture {
            fut: T::from_request(req, payload),
        }
    }
}

#[pin_project::pin_project]
pub struct FromRequestOptFuture<Fut> {
    #[pin]
    fut: Fut,
}

impl<Fut, T, E> Future for FromRequestOptFuture<Fut>
where
    Fut: Future<Output = Result<T, E>>,
    E: Into<Error>,
{
    type Output = Result<Option<T>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let res = ready!(this.fut.poll(cx));
        match res {
            Ok(t) => Poll::Ready(Ok(Some(t))),
            Err(e) => {
                log::debug!("Error for Option<T> extractor: {}", e.into());
                Poll::Ready(Ok(None))
            }
        }
    }
}

/// Optionally extract a field from the request or extract the Error if unsuccessful
///
/// If the `FromRequest` for T fails, inject Err into handler rather than returning an error response
///
/// # Examples
/// ```
/// use actix_web::{web, dev, App, Result, Error, HttpRequest, FromRequest};
/// use actix_web::error::ErrorBadRequest;
/// use futures_util::future::{ok, err, Ready};
/// use serde::Deserialize;
/// use rand;
///
/// #[derive(Debug, Deserialize)]
/// struct Thing {
///     name: String
/// }
///
/// impl FromRequest for Thing {
///     type Error = Error;
///     type Future = Ready<Result<Thing, Error>>;
///     type Config = ();
///
///     fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
///         if rand::random() {
///             ok(Thing { name: "thingy".into() })
///         } else {
///             err(ErrorBadRequest("no luck"))
///         }
///     }
/// }
///
/// /// extract `Thing` from request
/// async fn index(supplied_thing: Result<Thing>) -> String {
///     match supplied_thing {
///         Ok(thing) => format!("Got thing: {:?}", thing),
///         Err(e) => format!("Error extracting thing: {}", e)
///     }
/// }
///
/// fn main() {
///     let app = App::new().service(
///         web::resource("/users/:first").route(web::post().to(index))
///     );
/// }
/// ```
impl<T> FromRequest for Result<T, T::Error>
where
    T: FromRequest + 'static,
    T::Error: 'static,
    T::Future: 'static,
{
    type Error = Error;
    type Future = FromRequestResFuture<T::Future>;
    type Config = T::Config;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        FromRequestResFuture {
            fut: T::from_request(req, payload),
        }
    }
}

#[pin_project::pin_project]
pub struct FromRequestResFuture<Fut> {
    #[pin]
    fut: Fut,
}

impl<Fut, T, E> Future for FromRequestResFuture<Fut>
where
    Fut: Future<Output = Result<T, E>>,
{
    type Output = Result<Result<T, E>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let res = ready!(this.fut.poll(cx));
        Poll::Ready(Ok(res))
    }
}

/// Extract the request's URI.
///
/// # Examples
/// ```
/// use actix_web::{http::Uri, web, App, Responder};
///
/// async fn handler(uri: Uri) -> impl Responder {
///     format!("Requested path: {}", uri.path())
/// }
///
/// let app = App::new().default_service(web::to(handler));
/// ```
impl FromRequest for Uri {
    type Error = Infallible;
    type Future = Ready<Result<Self, Self::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ok(req.uri().clone())
    }
}

/// Extract the request's method.
///
/// # Examples
/// ```
/// use actix_web::{http::Method, web, App, Responder};
///
/// async fn handler(method: Method) -> impl Responder {
///     format!("Request method: {}", method)
/// }
///
/// let app = App::new().default_service(web::to(handler));
/// ```
impl FromRequest for Method {
    type Error = Infallible;
    type Future = Ready<Result<Self, Self::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ok(req.method().clone())
    }
}

#[doc(hidden)]
impl FromRequest for () {
    type Error = Infallible;
    type Future = Ready<Result<Self, Self::Error>>;
    type Config = ();

    fn from_request(_: &HttpRequest, _: &mut Payload) -> Self::Future {
        ok(())
    }
}

macro_rules! tuple_from_req ({$fut_type:ident, $(($n:tt, $T:ident)),+} => {

    // This module is a trick to get around the inability of
    // `macro_rules!` macros to make new idents. We want to make
    // a new `FutWrapper` struct for each distinct invocation of
    // this macro. Ideally, we would name it something like
    // `FutWrapper_$fut_type`, but this can't be done in a macro_rules
    // macro.
    //
    // Instead, we put everything in a module named `$fut_type`, thus allowing
    // us to use the name `FutWrapper` without worrying about conflicts.
    // This macro only exists to generate trait impls for tuples - these
    // are inherently global, so users don't have to care about this
    // weird trick.
    #[allow(non_snake_case)]
    mod $fut_type {

        // Bring everything into scope, so we don't need
        // redundant imports
        use super::*;

        /// A helper struct to allow us to pin-project through
        /// to individual fields
        #[pin_project::pin_project]
        struct FutWrapper<$($T: FromRequest),+>($(#[pin] MaybeDone<$T::Future>),+);

        /// FromRequest implementation for tuple
        #[doc(hidden)]
        #[allow(unused_parens)]
        impl<$($T: FromRequest + 'static),+> FromRequest for ($($T,)+)
        {
            type Error = Error;
            type Future = $fut_type<$($T),+>;
            type Config = ($($T::Config),+);

            fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
                $fut_type {
                    futs: FutWrapper($(maybe_done($T::from_request(req, payload)),)+),
                }
            }
        }

        #[doc(hidden)]
        #[pin_project::pin_project]
        pub struct $fut_type<$($T: FromRequest),+> {
            #[pin]
            futs: FutWrapper<$($T,)+>,
        }

        impl<$($T: FromRequest),+> Future for $fut_type<$($T),+>
        {
            type Output = Result<($($T,)+), Error>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = self.project();
                let mut futs = this.futs.project();

                $(
                    let mut fut = futs.$n.as_mut();

                    if fut.as_mut().output_mut().is_none() {
                        ready!(fut.as_mut().poll(cx));

                        // this is a bit hacky, but we want to first check if there was an error
                        // before taking the output.
                        if let Some(Err(_)) = fut.as_mut().output_mut() {
                            if let Some(Err(e)) = fut.take_output() {
                                return Poll::Ready(Err(e.into()));
                            }
                        }
                    }
                )+

                Poll::Ready(Ok(
                    ($(futs.$n.take_output().unwrap().unwrap_or_else(|_| unreachable!()),)+)
                ))
            }
        }
    }
});

#[rustfmt::skip]
mod m {
    use super::*;

    tuple_from_req!(TupleFromRequest1, (0, A));
    tuple_from_req!(TupleFromRequest2, (0, A), (1, B));
    tuple_from_req!(TupleFromRequest3, (0, A), (1, B), (2, C));
    tuple_from_req!(TupleFromRequest4, (0, A), (1, B), (2, C), (3, D));
    tuple_from_req!(TupleFromRequest5, (0, A), (1, B), (2, C), (3, D), (4, E));
    tuple_from_req!(TupleFromRequest6, (0, A), (1, B), (2, C), (3, D), (4, E), (5, F));
    tuple_from_req!(TupleFromRequest7, (0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G));
    tuple_from_req!(TupleFromRequest8, (0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G), (7, H));
    tuple_from_req!(TupleFromRequest9, (0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G), (7, H), (8, I));
    tuple_from_req!(TupleFromRequest10, (0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G), (7, H), (8, I), (9, J));
}

#[cfg(test)]
mod tests {
    use actix_http::http::header;
    use bytes::Bytes;
    use serde::Deserialize;

    use super::*;
    use crate::test::TestRequest;
    use crate::types::{Form, FormConfig};

    #[derive(Deserialize, Debug, PartialEq)]
    struct Info {
        hello: String,
    }

    #[actix_rt::test]
    async fn test_option() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .data(FormConfig::default().limit(4096))
            .to_http_parts();

        let r = Option::<Form<Info>>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(r, None);

        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((header::CONTENT_LENGTH, "9"))
            .set_payload(Bytes::from_static(b"hello=world"))
            .to_http_parts();

        let r = Option::<Form<Info>>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(
            r,
            Some(Form(Info {
                hello: "world".into()
            }))
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((header::CONTENT_LENGTH, "9"))
            .set_payload(Bytes::from_static(b"bye=world"))
            .to_http_parts();

        let r = Option::<Form<Info>>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(r, None);
    }

    #[actix_rt::test]
    async fn test_result() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((header::CONTENT_LENGTH, "11"))
            .set_payload(Bytes::from_static(b"hello=world"))
            .to_http_parts();

        let r = Result::<Form<Info>, Error>::from_request(&req, &mut pl)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            r,
            Form(Info {
                hello: "world".into()
            })
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, "application/x-www-form-urlencoded"))
            .insert_header((header::CONTENT_LENGTH, 9))
            .set_payload(Bytes::from_static(b"bye=world"))
            .to_http_parts();

        let r = Result::<Form<Info>, Error>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert!(r.is_err());
    }

    #[actix_rt::test]
    async fn test_uri() {
        let req = TestRequest::default().uri("/foo/bar").to_http_request();
        let uri = Uri::extract(&req).await.unwrap();
        assert_eq!(uri.path(), "/foo/bar");
    }

    #[actix_rt::test]
    async fn test_method() {
        let req = TestRequest::default().method(Method::GET).to_http_request();
        let method = Method::extract(&req).await.unwrap();
        assert_eq!(method, Method::GET);
    }
}
