//! Multipart payload support

use actix_utils::future::{ready, Ready};
use actix_web::{dev::Payload, Error, FromRequest, HttpRequest};

use crate::server::Multipart;

/// Get request's payload as multipart stream.
///
/// Content-type: multipart/form-data;
///
/// ## Server example
///
/// ```
/// use actix_web::{web, HttpResponse, Error};
/// use actix_multipart::Multipart;
/// use futures_util::stream::StreamExt as _;
///
/// async fn index(mut payload: Multipart) -> Result<HttpResponse, Error> {
///     // iterate over multipart stream
///     while let Some(item) = payload.next().await {
///            let mut field = item?;
///
///            // Field in turn is stream of *Bytes* object
///            while let Some(chunk) = field.next().await {
///                println!("-- CHUNK: \n{:?}", std::str::from_utf8(&chunk?));
///            }
///     }
///
///     Ok(HttpResponse::Ok().into())
/// }
/// ```
impl FromRequest for Multipart {
    type Error = Error;
    type Future = Ready<Result<Multipart, Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        ready(Ok(match Multipart::boundary(req.headers()) {
            Ok(boundary) => Multipart::from_boundary(boundary, payload.take()),
            Err(err) => Multipart::from_error(err),
        }))
    }
}
