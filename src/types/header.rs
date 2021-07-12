//! For header extractor helper documentation, see [`Header`](crate::types::Header).

use std::{fmt, ops};

use actix_utils::future::{err, ok, Ready};

use crate::{
    dev::Payload, error::ParseError, extract::FromRequest, http::header::Header as ParseHeader,
    HttpRequest,
};

/// Extract typed headers from the request.
///
/// To extract a header, the inner type `T` must implement the
/// [`Header`](crate::http::header::Header) trait.
///
/// # Examples
/// ```
/// use actix_web::{get, web, http::header};
///
/// #[get("/")]
/// async fn index(date: web::Header<header::Date>) -> String {
///     format!("Request was sent at {}", date.to_string())
/// }
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Header<T>(pub T);

impl<T> Header<T> {
    /// Unwrap into the inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> ops::Deref for Header<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Header<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> fmt::Display for Header<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<T> FromRequest for Header<T>
where
    T: ParseHeader,
{
    type Error = ParseError;
    type Future = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        match ParseHeader::parse(req) {
            Ok(header) => ok(Header(header)),
            Err(e) => err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{header, Method};
    use crate::test::TestRequest;

    #[actix_rt::test]
    async fn test_header_extract() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((header::CONTENT_TYPE, mime::APPLICATION_JSON))
            .insert_header((header::ALLOW, header::Allow(vec![Method::GET])))
            .to_http_parts();

        let s = Header::<header::ContentType>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.into_inner().0, mime::APPLICATION_JSON);

        let s = Header::<header::Allow>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.into_inner().0, vec![Method::GET]);

        assert!(Header::<header::Date>::from_request(&req, &mut pl)
            .await
            .is_err());
    }
}
