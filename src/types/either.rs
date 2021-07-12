//! For either helper, see [`Either`].

use std::{
    future::Future,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_core::ready;

use crate::{
    dev,
    web::{Form, Json},
    Error, FromRequest, HttpRequest, HttpResponse, Responder,
};

/// Combines two extractor or responder types into a single type.
///
/// Can be converted to and from an [`either::Either`].
///
/// # Extractor
/// Provides a mechanism for trying two extractors, a primary and a fallback. Useful for
/// "polymorphic payloads" where, for example, a form might be JSON or URL encoded.
///
/// It is important to note that this extractor, by necessity, buffers the entire request payload
/// as part of its implementation. Though, it does respect any `PayloadConfig` maximum size limits.
///
/// ```
/// use actix_web::{post, web, Either};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     name: String,
/// }
///
/// // handler that accepts form as JSON or form-urlencoded.
/// #[post("/")]
/// async fn index(form: Either<web::Json<Info>, web::Form<Info>>) -> String {
///     let name: String = match form {
///         Either::Left(json) => json.name.to_owned(),
///         Either::Right(form) => form.name.to_owned(),
///     };
///
///     format!("Welcome {}!", name)
/// }
/// ```
///
/// # Responder
/// It may be desireable to use a concrete type for a response with multiple branches. As long as
/// both types implement `Responder`, so will the `Either` type, enabling it to be used as a
/// handler's return type.
///
/// All properties of a response are determined by the Responder branch returned.
///
/// ```
/// use actix_web::{get, Either, Error, HttpResponse};
///
/// #[get("/")]
/// async fn index() -> Either<&'static str, Result<HttpResponse, Error>> {
///     if 1 == 2 {
///         // respond with Left variant
///         Either::Left("Bad data")
///     } else {
///         // respond with Right variant
///         Either::Right(
///             Ok(HttpResponse::Ok()
///                 .content_type(mime::TEXT_HTML)
///                 .body("<p>Hello!</p>"))
///         )
///     }
/// }
/// ```
#[derive(Debug, PartialEq)]
pub enum Either<L, R> {
    /// A value of type `L`.
    Left(L),

    /// A value of type `R`.
    Right(R),
}

impl<T> Either<Form<T>, Json<T>> {
    pub fn into_inner(self) -> T {
        match self {
            Either::Left(form) => form.into_inner(),
            Either::Right(form) => form.into_inner(),
        }
    }
}

impl<T> Either<Json<T>, Form<T>> {
    pub fn into_inner(self) -> T {
        match self {
            Either::Left(form) => form.into_inner(),
            Either::Right(form) => form.into_inner(),
        }
    }
}

impl<L, R> From<either::Either<L, R>> for Either<L, R> {
    fn from(val: either::Either<L, R>) -> Self {
        match val {
            either::Either::Left(l) => Either::Left(l),
            either::Either::Right(r) => Either::Right(r),
        }
    }
}

impl<L, R> From<Either<L, R>> for either::Either<L, R> {
    fn from(val: Either<L, R>) -> Self {
        match val {
            Either::Left(l) => either::Either::Left(l),
            Either::Right(r) => either::Either::Right(r),
        }
    }
}

#[cfg(test)]
impl<L, R> Either<L, R> {
    pub(self) fn unwrap_left(self) -> L {
        match self {
            Either::Left(data) => data,
            Either::Right(_) => {
                panic!("Cannot unwrap Left branch. Either contains an `R` type.")
            }
        }
    }

    pub(self) fn unwrap_right(self) -> R {
        match self {
            Either::Left(_) => {
                panic!("Cannot unwrap Right branch. Either contains an `L` type.")
            }
            Either::Right(data) => data,
        }
    }
}

/// See [here](#responder) for example of usage as a handler return type.
impl<L, R> Responder for Either<L, R>
where
    L: Responder,
    R: Responder,
{
    fn respond_to(self, req: &HttpRequest) -> HttpResponse {
        match self {
            Either::Left(a) => a.respond_to(req),
            Either::Right(b) => b.respond_to(req),
        }
    }
}

/// A composite error resulting from failure to extract an `Either<L, R>`.
///
/// The implementation of `Into<actix_web::Error>` will return the payload buffering error or the
/// error from the primary extractor. To access the fallback error, use a match clause.
#[derive(Debug)]
pub enum EitherExtractError<L, R> {
    /// Error from payload buffering, such as exceeding payload max size limit.
    Bytes(Error),

    /// Error from primary extractor.
    Extract(L, R),
}

impl<L, R> From<EitherExtractError<L, R>> for Error
where
    L: Into<Error>,
    R: Into<Error>,
{
    fn from(err: EitherExtractError<L, R>) -> Error {
        match err {
            EitherExtractError::Bytes(err) => err,
            EitherExtractError::Extract(a_err, _b_err) => a_err.into(),
        }
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<L, R> FromRequest for Either<L, R>
where
    L: FromRequest + 'static,
    R: FromRequest + 'static,
{
    type Error = EitherExtractError<L::Error, R::Error>;
    type Future = EitherExtractFut<L, R>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        EitherExtractFut {
            req: req.clone(),
            state: EitherExtractState::Bytes {
                bytes: Bytes::from_request(req, payload),
            },
        }
    }
}

#[pin_project::pin_project]
pub struct EitherExtractFut<L, R>
where
    R: FromRequest,
    L: FromRequest,
{
    req: HttpRequest,
    #[pin]
    state: EitherExtractState<L, R>,
}

#[pin_project::pin_project(project = EitherExtractProj)]
pub enum EitherExtractState<L, R>
where
    L: FromRequest,
    R: FromRequest,
{
    Bytes {
        #[pin]
        bytes: <Bytes as FromRequest>::Future,
    },
    Left {
        #[pin]
        left: L::Future,
        fallback: Bytes,
    },
    Right {
        #[pin]
        right: R::Future,
        left_err: Option<L::Error>,
    },
}

impl<R, RF, RE, L, LF, LE> Future for EitherExtractFut<L, R>
where
    L: FromRequest<Future = LF, Error = LE>,
    R: FromRequest<Future = RF, Error = RE>,
    LF: Future<Output = Result<L, LE>> + 'static,
    RF: Future<Output = Result<R, RE>> + 'static,
    LE: Into<Error>,
    RE: Into<Error>,
{
    type Output = Result<Either<L, R>, EitherExtractError<LE, RE>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let ready = loop {
            let next = match this.state.as_mut().project() {
                EitherExtractProj::Bytes { bytes } => {
                    let res = ready!(bytes.poll(cx));
                    match res {
                        Ok(bytes) => {
                            let fallback = bytes.clone();
                            let left =
                                L::from_request(&this.req, &mut payload_from_bytes(bytes));
                            EitherExtractState::Left { left, fallback }
                        }
                        Err(err) => break Err(EitherExtractError::Bytes(err)),
                    }
                }
                EitherExtractProj::Left { left, fallback } => {
                    let res = ready!(left.poll(cx));
                    match res {
                        Ok(extracted) => break Ok(Either::Left(extracted)),
                        Err(left_err) => {
                            let right = R::from_request(
                                &this.req,
                                &mut payload_from_bytes(mem::take(fallback)),
                            );
                            EitherExtractState::Right {
                                left_err: Some(left_err),
                                right,
                            }
                        }
                    }
                }
                EitherExtractProj::Right { right, left_err } => {
                    let res = ready!(right.poll(cx));
                    match res {
                        Ok(data) => break Ok(Either::Right(data)),
                        Err(err) => {
                            break Err(EitherExtractError::Extract(
                                left_err.take().unwrap(),
                                err,
                            ));
                        }
                    }
                }
            };
            this.state.set(next);
        };
        Poll::Ready(ready)
    }
}

fn payload_from_bytes(bytes: Bytes) -> dev::Payload {
    let (_, mut h1_payload) = actix_http::h1::Payload::create(true);
    h1_payload.unread_data(bytes);
    dev::Payload::from(h1_payload)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{
        test::TestRequest,
        web::{Form, Json},
    };

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestForm {
        hello: String,
    }

    #[actix_rt::test]
    async fn test_either_extract_first_try() {
        let (req, mut pl) = TestRequest::default()
            .set_form(&TestForm {
                hello: "world".to_owned(),
            })
            .to_http_parts();

        let form = Either::<Form<TestForm>, Json<TestForm>>::from_request(&req, &mut pl)
            .await
            .unwrap()
            .unwrap_left()
            .into_inner();
        assert_eq!(&form.hello, "world");
    }

    #[actix_rt::test]
    async fn test_either_extract_fallback() {
        let (req, mut pl) = TestRequest::default()
            .set_json(&TestForm {
                hello: "world".to_owned(),
            })
            .to_http_parts();

        let form = Either::<Form<TestForm>, Json<TestForm>>::from_request(&req, &mut pl)
            .await
            .unwrap()
            .unwrap_right()
            .into_inner();
        assert_eq!(&form.hello, "world");
    }

    #[actix_rt::test]
    async fn test_either_extract_recursive_fallback() {
        let (req, mut pl) = TestRequest::default()
            .set_payload(Bytes::from_static(b"!@$%^&*()"))
            .to_http_parts();

        let payload = Either::<Either<Form<TestForm>, Json<TestForm>>, Bytes>::from_request(
            &req, &mut pl,
        )
        .await
        .unwrap()
        .unwrap_right();
        assert_eq!(&payload.as_ref(), &b"!@$%^&*()");
    }

    #[actix_rt::test]
    async fn test_either_extract_recursive_fallback_inner() {
        let (req, mut pl) = TestRequest::default()
            .set_json(&TestForm {
                hello: "world".to_owned(),
            })
            .to_http_parts();

        let form = Either::<Either<Form<TestForm>, Json<TestForm>>, Bytes>::from_request(
            &req, &mut pl,
        )
        .await
        .unwrap()
        .unwrap_left()
        .unwrap_right()
        .into_inner();
        assert_eq!(&form.hello, "world");
    }
}
