use std::{any::type_name, ops::Deref};

use actix_utils::future::{err, ok, Ready};

use crate::{dev::Payload, error::ErrorInternalServerError, Error, FromRequest, HttpRequest};

/// Request-local data extractor.
///
/// Request-local data is arbitrary data attached to an individual request, usually
/// by middleware. It can be set via `extensions_mut` on [`HttpRequest`][htr_ext_mut]
/// or [`ServiceRequest`][srv_ext_mut].
///
/// Unlike app data, request data is dropped when the request has finished processing. This makes it
/// useful as a kind of messaging system between middleware and request handlers. It uses the same
/// types-as-keys storage system as app data.
///
/// # Mutating Request Data
/// Note that since extractors must output owned data, only types that `impl Clone` can use this
/// extractor. A clone is taken of the required request data and can, therefore, not be directly
/// mutated in-place. To mutate request data, continue to use [`HttpRequest::extensions_mut`] or
/// re-insert the cloned data back into the extensions map. A `DerefMut` impl is intentionally not
/// provided to make this potential foot-gun more obvious.
///
/// # Example
/// ```no_run
/// # use actix_web::{web, HttpResponse, HttpRequest, Responder};
///
/// #[derive(Debug, Clone, PartialEq)]
/// struct FlagFromMiddleware(String);
///
/// /// Use the `ReqData<T>` extractor to access request data in a handler.
/// async fn handler(
///     req: HttpRequest,
///     opt_flag: Option<web::ReqData<FlagFromMiddleware>>,
/// ) -> impl Responder {
///     // use an optional extractor if the middleware is
///     // not guaranteed to add this type of requests data
///     if let Some(flag) = opt_flag {
///         assert_eq!(&flag.into_inner(), req.extensions().get::<FlagFromMiddleware>().unwrap());
///     }
///     
///     HttpResponse::Ok()
/// }
/// ```
///
/// [htr_ext_mut]: crate::HttpRequest::extensions_mut
/// [srv_ext_mut]: crate::dev::ServiceRequest::extensions_mut
#[derive(Debug, Clone)]
pub struct ReqData<T: Clone + 'static>(T);

impl<T: Clone + 'static> ReqData<T> {
    /// Consumes the `ReqData`, returning its wrapped data.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone + 'static> Deref for ReqData<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone + 'static> FromRequest for ReqData<T> {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if let Some(st) = req.extensions().get::<T>() {
            ok(ReqData(st.clone()))
        } else {
            log::debug!(
                "Failed to construct App-level ReqData extractor. \
                 Request path: {:?} (type: {})",
                req.path(),
                type_name::<T>(),
            );
            err(ErrorInternalServerError(
                "Missing expected request extension data",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use futures_util::TryFutureExt as _;

    use super::*;
    use crate::{
        dev::Service,
        http::{Method, StatusCode},
        test::{init_service, TestRequest},
        web, App, HttpMessage, HttpResponse,
    };

    #[actix_rt::test]
    async fn req_data_extractor() {
        let srv = init_service(
            App::new()
                .wrap_fn(|req, srv| {
                    if req.method() == Method::POST {
                        req.extensions_mut().insert(42u32);
                    }

                    srv.call(req)
                })
                .service(web::resource("/test").to(
                    |req: HttpRequest, data: Option<ReqData<u32>>| {
                        if req.method() != Method::POST {
                            assert!(data.is_none());
                        }

                        if let Some(data) = data {
                            assert_eq!(*data, 42);
                            assert_eq!(
                                Some(data.into_inner()),
                                req.extensions().get::<u32>().copied()
                            );
                        }

                        HttpResponse::Ok()
                    },
                )),
        )
        .await;

        let req = TestRequest::get().uri("/test").to_request();
        let resp = srv.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let req = TestRequest::post().uri("/test").to_request();
        let resp = srv.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_rt::test]
    async fn req_data_internal_mutability() {
        let srv = init_service(
            App::new()
                .wrap_fn(|req, srv| {
                    let data_before = Rc::new(RefCell::new(42u32));
                    req.extensions_mut().insert(data_before);

                    srv.call(req).map_ok(|res| {
                        {
                            let ext = res.request().extensions();
                            let data_after = ext.get::<Rc<RefCell<u32>>>().unwrap();
                            assert_eq!(*data_after.borrow(), 53u32);
                        }

                        res
                    })
                })
                .default_service(web::to(|data: ReqData<Rc<RefCell<u32>>>| {
                    assert_eq!(*data.borrow(), 42);
                    *data.borrow_mut() += 11;
                    assert_eq!(*data.borrow(), 53);

                    HttpResponse::Ok()
                })),
        )
        .await;

        let req = TestRequest::get().uri("/test").to_request();
        let resp = srv.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
