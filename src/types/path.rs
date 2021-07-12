//! For path segment extractor documentation, see [`Path`].

use std::{fmt, ops, sync::Arc};

use actix_router::PathDeserializer;
use actix_utils::future::{ready, Ready};
use serde::de;

use crate::{
    dev::Payload,
    error::{Error, ErrorNotFound, PathError},
    FromRequest, HttpRequest,
};

/// Extract typed data from request path segments.
///
/// Use [`PathConfig`] to configure extraction process.
///
/// # Examples
/// ```
/// use actix_web::{get, web};
///
/// // extract path info from "/{name}/{count}/index.html" into tuple
/// // {name}  - deserialize a String
/// // {count} - deserialize a u32
/// #[get("/{name}/{count}/index.html")]
/// async fn index(path: web::Path<(String, u32)>) -> String {
///     let (name, count) = path.into_inner();
///     format!("Welcome {}! {}", name, count)
/// }
/// ```
///
/// Path segments also can be deserialized into any type that implements [`serde::Deserialize`].
/// Path segment labels will be matched with struct field names.
///
/// ```
/// use actix_web::{get, web};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     name: String,
/// }
///
/// // extract `Info` from a path using serde
/// #[get("/{name}")]
/// async fn index(info: web::Path<Info>) -> String {
///     format!("Welcome {}!", info.name)
/// }
/// ```
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Path<T>(T);

impl<T> Path<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> AsRef<T> for Path<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::Deref for Path<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Path<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> From<T> for Path<T> {
    fn from(inner: T) -> Path<T> {
        Path(inner)
    }
}

impl<T: fmt::Display> fmt::Display for Path<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// See [here](#usage) for example of usage as an extractor.
impl<T> FromRequest for Path<T>
where
    T: de::DeserializeOwned,
{
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let error_handler = req
            .app_data::<PathConfig>()
            .and_then(|c| c.ehandler.clone());

        ready(
            de::Deserialize::deserialize(PathDeserializer::new(req.match_info()))
                .map(Path)
                .map_err(move |err| {
                    log::debug!(
                        "Failed during Path extractor deserialization. \
                         Request path: {:?}",
                        req.path()
                    );
                    if let Some(error_handler) = error_handler {
                        let e = PathError::Deserialize(err);
                        (error_handler)(e, req)
                    } else {
                        ErrorNotFound(err)
                    }
                }),
        )
    }
}

/// Path extractor configuration
///
/// ```
/// use actix_web::web::PathConfig;
/// use actix_web::{error, web, App, FromRequest, HttpResponse};
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Debug)]
/// enum Folder {
///     #[serde(rename = "inbox")]
///     Inbox,
///     #[serde(rename = "outbox")]
///     Outbox,
/// }
///
/// // deserialize `Info` from request's path
/// async fn index(folder: web::Path<Folder>) -> String {
///     format!("Selected folder: {:?}!", folder)
/// }
///
/// fn main() {
///     let app = App::new().service(
///         web::resource("/messages/{folder}")
///             .app_data(PathConfig::default().error_handler(|err, req| {
///                 error::InternalError::from_response(
///                     err,
///                     HttpResponse::Conflict().into(),
///                 )
///                 .into()
///             }))
///             .route(web::post().to(index)),
///     );
/// }
/// ```
#[derive(Clone)]
pub struct PathConfig {
    ehandler: Option<Arc<dyn Fn(PathError, &HttpRequest) -> Error + Send + Sync>>,
}

impl PathConfig {
    /// Set custom error handler
    pub fn error_handler<F>(mut self, f: F) -> Self
    where
        F: Fn(PathError, &HttpRequest) -> Error + Send + Sync + 'static,
    {
        self.ehandler = Some(Arc::new(f));
        self
    }
}

impl Default for PathConfig {
    fn default() -> Self {
        PathConfig { ehandler: None }
    }
}

#[cfg(test)]
mod tests {
    use actix_router::ResourceDef;
    use derive_more::Display;
    use serde::Deserialize;

    use super::*;
    use crate::test::TestRequest;
    use crate::{error, http, HttpResponse};

    #[derive(Deserialize, Debug, Display)]
    #[display(fmt = "MyStruct({}, {})", key, value)]
    struct MyStruct {
        key: String,
        value: String,
    }

    #[derive(Deserialize)]
    struct Test2 {
        key: String,
        value: u32,
    }

    #[actix_rt::test]
    async fn test_extract_path_single() {
        let resource = ResourceDef::new("/{value}/");

        let mut req = TestRequest::with_uri("/32/").to_srv_request();
        resource.match_path(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        assert_eq!(*Path::<i8>::from_request(&req, &mut pl).await.unwrap(), 32);
        assert!(Path::<MyStruct>::from_request(&req, &mut pl).await.is_err());
    }

    #[actix_rt::test]
    async fn test_tuple_extract() {
        let resource = ResourceDef::new("/{key}/{value}/");

        let mut req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();
        resource.match_path(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let (Path(res),) = <(Path<(String, String)>,)>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(res.0, "name");
        assert_eq!(res.1, "user1");

        let (Path(a), Path(b)) =
            <(Path<(String, String)>, Path<(String, String)>)>::from_request(&req, &mut pl)
                .await
                .unwrap();
        assert_eq!(a.0, "name");
        assert_eq!(a.1, "user1");
        assert_eq!(b.0, "name");
        assert_eq!(b.1, "user1");

        let () = <()>::from_request(&req, &mut pl).await.unwrap();
    }

    #[actix_rt::test]
    async fn test_request_extract() {
        let mut req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();

        let resource = ResourceDef::new("/{key}/{value}/");
        resource.match_path(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let mut s = Path::<MyStruct>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.key, "name");
        assert_eq!(s.value, "user1");
        s.value = "user2".to_string();
        assert_eq!(s.value, "user2");
        assert_eq!(
            format!("{}, {:?}", s, s),
            "MyStruct(name, user2), Path(MyStruct { key: \"name\", value: \"user2\" })"
        );
        let s = s.into_inner();
        assert_eq!(s.value, "user2");

        let Path(s) = Path::<(String, String)>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.0, "name");
        assert_eq!(s.1, "user1");

        let mut req = TestRequest::with_uri("/name/32/").to_srv_request();
        let resource = ResourceDef::new("/{key}/{value}/");
        resource.match_path(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let s = Path::<Test2>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.as_ref().key, "name");
        assert_eq!(s.value, 32);

        let Path(s) = Path::<(String, u8)>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.0, "name");
        assert_eq!(s.1, 32);

        let res = Path::<Vec<String>>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(res[0], "name".to_owned());
        assert_eq!(res[1], "32".to_owned());
    }

    #[actix_rt::test]
    async fn test_custom_err_handler() {
        let (req, mut pl) = TestRequest::with_uri("/name/user1/")
            .app_data(PathConfig::default().error_handler(|err, _| {
                error::InternalError::from_response(err, HttpResponse::Conflict().finish())
                    .into()
            }))
            .to_http_parts();

        let s = Path::<(usize,)>::from_request(&req, &mut pl)
            .await
            .unwrap_err();
        let res = HttpResponse::from_error(s);

        assert_eq!(res.status(), http::StatusCode::CONFLICT);
    }
}
